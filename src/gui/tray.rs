//! System tray icon: left click opens the window, the menu switches profile or quits.
//!
//! Linux uses the StatusNotifierItem D-Bus protocol (KDE, and GNOME with the
//! AppIndicator extension); Windows uses a notification-area icon.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::i18n::t;

pub enum TrayCmd {
    Open,
    Apply(String),
    Quit,
}

/// Wakes the GUI (or the hidden-window loop) after a command is sent.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

fn icon_rgba() -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(include_bytes!("../../assets/icon-64.png"))
        .expect("embedded icon")
        .to_rgba8();
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}

fn send(tx: &Sender<TrayCmd>, wake: &Wake, cmd: TrayCmd) {
    let _ = tx.send(cmd);
    wake();
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use ksni::blocking::TrayMethods;
    use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
    use ksni::MenuItem;

    struct AkTray {
        tx: Sender<TrayCmd>,
        wake: Wake,
        profiles: Vec<String>,
        active: Option<String>,
        icon: ksni::Icon,
    }

    impl ksni::Tray for AkTray {
        fn id(&self) -> String {
            "ak820".into()
        }
        fn title(&self) -> String {
            "AK820 Pro".into()
        }
        fn icon_name(&self) -> String {
            "ak820".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![self.icon.clone()]
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "AK820 Pro".into(),
                description: self.active.clone().unwrap_or_default(),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            send(&self.tx, &self.wake, TrayCmd::Open);
        }
        fn menu(&self) -> Vec<MenuItem<Self>> {
            let profiles: Vec<MenuItem<Self>> = if self.profiles.is_empty() {
                vec![StandardItem { label: t("(no saved profiles)", "(nessun profilo salvato)").into(), enabled: false, ..Default::default() }.into()]
            } else {
                self.profiles
                    .iter()
                    .map(|name| {
                        let n = name.clone();
                        CheckmarkItem {
                            label: name.replace('_', "__"),
                            checked: self.active.as_deref() == Some(name),
                            activate: Box::new(move |this: &mut Self| send(&this.tx, &this.wake, TrayCmd::Apply(n.clone()))),
                            ..Default::default()
                        }
                        .into()
                    })
                    .collect()
            };
            vec![
                StandardItem {
                    label: t("Open AK820 Pro", "Apri AK820 Pro").into(),
                    activate: Box::new(|this: &mut Self| send(&this.tx, &this.wake, TrayCmd::Open)),
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                SubMenu { label: t("Profiles", "Profili").into(), submenu: profiles, ..Default::default() }.into(),
                MenuItem::Separator,
                StandardItem {
                    label: t("Quit", "Esci").into(),
                    icon_name: "application-exit".into(),
                    activate: Box::new(|this: &mut Self| send(&this.tx, &this.wake, TrayCmd::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    pub struct Handle(ksni::blocking::Handle<AkTray>);

    impl Handle {
        pub fn set_profiles(&self, profiles: Vec<String>, active: Option<String>) {
            self.0.update(|t| {
                t.profiles = profiles;
                t.active = active;
            });
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            self.0.shutdown().wait();
        }
    }

    pub fn spawn(tx: Sender<TrayCmd>, wake: Wake, profiles: Vec<String>, active: Option<String>) -> Option<Handle> {
        let (rgba, w, h) = icon_rgba();
        // ARGB32 in network byte order.
        let data = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
        let icon = ksni::Icon { width: w as i32, height: h as i32, data };
        AkTray { tx, wake, profiles, active, icon }.spawn().ok().map(Handle)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::{mpsc, Mutex};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_APP, WM_QUIT,
    };

    #[derive(Default)]
    struct Ids {
        open: Option<MenuId>,
        quit: Option<MenuId>,
        profiles: Vec<(MenuId, String)>,
    }

    fn build_menu(profiles: &[String], active: &Option<String>) -> (Menu, Ids) {
        let menu = Menu::new();
        let open = MenuItem::new(t("Open AK820 Pro", "Apri AK820 Pro"), true, None);
        let sub = Submenu::new(t("Profiles", "Profili"), true);
        let mut ids = Ids::default();
        if profiles.is_empty() {
            let _ = sub.append(&MenuItem::new(t("(no saved profiles)", "(nessun profilo salvato)"), false, None));
        }
        for name in profiles {
            let item = CheckMenuItem::new(name, true, active.as_deref() == Some(name), None);
            ids.profiles.push((item.id().clone(), name.clone()));
            let _ = sub.append(&item);
        }
        let quit = MenuItem::new(t("Quit", "Esci"), true, None);
        let _ = menu.append(&open);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&sub);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);
        ids.open = Some(open.id().clone());
        ids.quit = Some(quit.id().clone());
        (menu, ids)
    }

    pub struct Handle {
        thread_id: u32,
        updates: mpsc::Sender<(Vec<String>, Option<String>)>,
    }

    impl Handle {
        pub fn set_profiles(&self, profiles: Vec<String>, active: Option<String>) {
            let _ = self.updates.send((profiles, active));
            // SAFETY: posting a message to a thread id we own is always sound.
            unsafe { PostThreadMessageW(self.thread_id, WM_APP, 0, 0) };
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: as above; WM_QUIT ends the tray thread's message loop.
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        }
    }

    pub fn spawn(tx: Sender<TrayCmd>, wake: Wake, profiles: Vec<String>, active: Option<String>) -> Option<Handle> {
        let (updates, update_rx) = mpsc::channel::<(Vec<String>, Option<String>)>();
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let ids = Arc::new(Mutex::new(Ids::default()));
            let (menu, new_ids) = build_menu(&profiles, &active);
            *ids.lock().unwrap() = new_ids;
            let (rgba, w, h) = icon_rgba();
            let tray = Icon::from_rgba(rgba, w, h).ok().and_then(|icon| {
                TrayIconBuilder::new()
                    .with_menu(Box::new(menu))
                    .with_tooltip("AK820 Pro")
                    .with_icon(icon)
                    .build()
                    .ok()
            });
            let Some(tray) = tray else {
                let _ = ready_tx.send(None);
                return;
            };
            let (menu_tx, menu_wake, menu_ids) = (tx.clone(), wake.clone(), ids.clone());
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let ids = menu_ids.lock().unwrap();
                let cmd = if ids.open.as_ref() == Some(&e.id) {
                    Some(TrayCmd::Open)
                } else if ids.quit.as_ref() == Some(&e.id) {
                    Some(TrayCmd::Quit)
                } else {
                    ids.profiles.iter().find(|(id, _)| *id == e.id).map(|(_, n)| TrayCmd::Apply(n.clone()))
                };
                if let Some(c) = cmd {
                    send(&menu_tx, &menu_wake, c);
                }
            }));
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                    send(&tx, &wake, TrayCmd::Open);
                }
            }));
            // SAFETY: plain Win32 calls on this thread.
            let _ = ready_tx.send(Some(unsafe { GetCurrentThreadId() }));
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                if msg.message == WM_APP {
                    while let Ok((p, a)) = update_rx.try_recv() {
                        let (menu, new_ids) = build_menu(&p, &a);
                        *ids.lock().unwrap() = new_ids;
                        tray.set_menu(Some(Box::new(menu)));
                    }
                    continue;
                }
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        let thread_id = ready_rx.recv().ok()??;
        Some(Handle { thread_id, updates })
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use super::*;

    pub struct Handle;

    impl Handle {
        pub fn set_profiles(&self, _profiles: Vec<String>, _active: Option<String>) {}
    }

    pub fn spawn(_tx: Sender<TrayCmd>, _wake: Wake, _profiles: Vec<String>, _active: Option<String>) -> Option<Handle> {
        None
    }
}

pub use imp::{spawn, Handle};
