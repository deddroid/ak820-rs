//! File drag & drop on native Wayland, which winit 0.30 lacks. A second client on the
//! window's Wayland connection (as smithay-clipboard does for the clipboard) listens to
//! the seat's data device and hands the dropped `text/uri-list` to the app. On X11 and
//! Windows, winit delivers drops itself.

/// What the window sees of the current drag.
#[derive(Default)]
pub struct Drops {
    /// Files are being dragged over the window.
    pub hovering: bool,
    /// Lines of the URI lists dropped since the last [`DropTarget::take`].
    pub dropped: Vec<String>,
}

#[cfg(target_os = "linux")]
pub use linux::DropTarget;

/// No-op where winit already handles drops.
#[cfg(not(target_os = "linux"))]
pub struct DropTarget;

#[cfg(not(target_os = "linux"))]
impl DropTarget {
    pub fn new<W>(_: &W, _: crate::tray::Wake) -> Option<Self> {
        None
    }

    pub fn take(&self) -> Drops {
        Drops::default()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
    use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
    use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
    use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
    use smithay_client_toolkit::reexports::calloop::{channel, EventLoop};
    use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
    use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
    use smithay_client_toolkit::reexports::client::protocol::wl_data_device::WlDataDevice;
    use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction;
    use smithay_client_toolkit::reexports::client::protocol::wl_data_source::WlDataSource;
    use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
    use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
    use smithay_client_toolkit::reexports::client::{Connection, QueueHandle};
    use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
    use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
    use smithay_client_toolkit::{delegate_data_device, delegate_registry, delegate_seat, registry_handlers};
    use wayland_backend::client::Backend;

    use super::Drops;
    use crate::tray::Wake;

    const URI_LIST: &str = "text/uri-list";

    /// Listens for drops while it lives; dropping it stops and joins the worker.
    pub struct DropTarget {
        drops: Arc<Mutex<Drops>>,
        exit: channel::Sender<()>,
        thread: Option<JoinHandle<()>>,
    }

    impl DropTarget {
        /// `None` when the window is not on Wayland.
        ///
        /// The Wayland display must outlive the target: eframe keeps one event loop (and
        /// its connection) for the whole program, so the app keeps the target until `main`
        /// returns.
        pub fn new(window: &impl HasDisplayHandle, wake: Wake) -> Option<Self> {
            let RawDisplayHandle::Wayland(handle) = window.display_handle().ok()?.as_raw() else {
                return None;
            };
            // SAFETY: the pointer is eframe's live `wl_display`, which outlives this target
            // (see above); libwayland allows several queues on one display.
            let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
            let conn = Connection::from_backend(backend);
            let drops = Arc::<Mutex<Drops>>::default();
            let (exit, exit_rx) = channel::channel();
            let shared = drops.clone();
            let thread = std::thread::Builder::new()
                .name("wayland-drop".into())
                .spawn(move || run(conn, exit_rx, shared, wake))
                .ok()?;
            Some(Self { drops, exit, thread: Some(thread) })
        }

        pub fn take(&self) -> Drops {
            let mut d = self.drops.lock().expect("drop state lock");
            Drops { hovering: d.hovering, dropped: std::mem::take(&mut d.dropped) }
        }
    }

    impl Drop for DropTarget {
        fn drop(&mut self) {
            // The worker may already be gone (no data device manager): nothing to stop then.
            let _ = self.exit.send(());
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    struct State {
        registry: RegistryState,
        seats: SeatState,
        manager: DataDeviceManagerState,
        devices: Vec<DataDevice>,
        drops: Arc<Mutex<Drops>>,
        wake: Wake,
        exit: bool,
    }

    fn run(conn: Connection, exit: channel::Channel<()>, drops: Arc<Mutex<Drops>>, wake: Wake) {
        let Ok((globals, queue)) = registry_queue_init::<State>(&conn) else { return };
        let qh = queue.handle();
        let Ok(manager) = DataDeviceManagerState::bind(&globals, &qh) else { return };
        let Ok(mut event_loop) = EventLoop::<State>::try_new() else { return };
        let seats = SeatState::new(&globals, &qh);
        let devices = seats.seats().map(|seat| manager.get_data_device(&qh, &seat)).collect();
        let mut state =
            State { registry: RegistryState::new(&globals), seats, manager, devices, drops, wake, exit: false };
        let handle = event_loop.handle();
        if handle.insert_source(exit, |_, _, state| state.exit = true).is_err()
            || WaylandSource::new(conn, queue).insert(handle).is_err()
        {
            return;
        }
        while !state.exit && event_loop.dispatch(None, &mut state).is_ok() {}
    }

    impl State {
        fn offer(&self, device: &WlDataDevice) -> Option<DragOffer> {
            self.devices.iter().find(|d| d.inner() == device)?.data().drag_offer()
        }

        fn update(&self, f: impl FnOnce(&mut Drops)) {
            f(&mut self.drops.lock().expect("drop state lock"));
            (self.wake)();
        }
    }

    impl DataDeviceHandler for State {
        fn enter(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            device: &WlDataDevice,
            _: f64,
            _: f64,
            _: &WlSurface,
        ) {
            let Some(offer) = self.offer(device) else { return };
            let files = offer.with_mime_types(|types| types.iter().any(|t| t == URI_LIST));
            offer.accept_mime_type(offer.serial, files.then(|| URI_LIST.to_string()));
            if files {
                offer.set_actions(DndAction::Copy, DndAction::Copy);
                self.update(|d| d.hovering = true);
            }
        }

        fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
            self.update(|d| d.hovering = false);
        }

        fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

        fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

        fn drop_performed(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
            let Some(offer) = self.offer(device) else { return };
            let mut list = String::new();
            // The request must reach the source before reading, or the read never ends.
            // A failed transfer only loses this drop.
            if let Ok(mut pipe) = offer.receive(URI_LIST.into()) {
                if conn.flush().is_ok() {
                    let _ = pipe.read_to_string(&mut list);
                }
            }
            offer.finish();
            offer.destroy();
            self.update(|d| {
                d.hovering = false;
                d.dropped.extend(list.lines().filter(|l| !l.starts_with('#')).map(str::to_string));
            });
        }
    }

    impl DataOfferHandler for State {
        fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}

        fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    }

    /// Required by the data device delegate; this client never offers data.
    impl DataSourceHandler for State {
        fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

        fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, _: WritePipe) {}

        fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

        fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

        fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

        fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
    }

    impl SeatHandler for State {
        fn seat_state(&mut self) -> &mut SeatState {
            &mut self.seats
        }

        fn new_seat(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: WlSeat) {
            if !self.devices.iter().any(|d| d.data().seat() == &seat) {
                self.devices.push(self.manager.get_data_device(qh, &seat));
            }
        }

        fn new_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, _: Capability) {}

        fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, _: Capability) {}

        fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
            self.devices.retain(|d| d.data().seat() != &seat);
        }
    }

    impl ProvidesRegistryState for State {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry
        }
        registry_handlers![SeatState];
    }

    delegate_seat!(State);
    delegate_data_device!(State);
    delegate_registry!(State);
}
