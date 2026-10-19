"""Fail unless every image of assets/icon.ico is embedded in the given Windows executables."""
import struct
import sys

ico = open("assets/icon.ico", "rb").read()
_, _, count = struct.unpack_from("<HHH", ico, 0)
images = []
for i in range(count):
    size, offset = struct.unpack_from("<II", ico, 6 + 16 * i + 8)
    images.append(ico[offset:offset + size])

failed = False
for exe in sys.argv[1:]:
    data = open(exe, "rb").read()
    missing = sum(img not in data for img in images)
    print(f"{exe}: {count - missing}/{count} icon images embedded")
    failed |= missing > 0
sys.exit(1 if failed else 0)
