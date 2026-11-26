# ArchiveCraft behaviour sheet

Every number and default that shapes how ArchiveCraft behaves, with where it came from. Clean room: values are taken from the reference screenshots (layout and defaults only) or are our own estimates, marked as such.

| Group | Setting | Value | Source |
|---|---|---|---|
| packing | default compression level | 5 | own choice; reference/02-add-general.png shows a middle setting as default (estimated) |
| packing | compression levels offered | 0,1,3,5,7,9 | own choice (estimated) |
| packing | default archive type | ZIP | own choice (the target's own format is out of scope, reference/README.md) |
| packing | default update mode | add and replace | reference/02-add-general.png, reference/51-add-update-mode-list.png |
| packing | update modes | add&replace, update newer, freshen, ask, skip existing, mirror | reference/51-add-update-mode-list.png |
| packing | default path mode | relative | reference/05-add-files.png |
| packing | store modification time | True | reference/07-add-time.png |
| packing | default recovery data | 3 % | reference/03-add-advanced.png |
| packing | PAR2 target slice count | 2000 slices | estimated |
| packing | PAR2 minimum slice size | 512 bytes | estimated |
| packing | split size presets | 10 MB, 100 MB, 700 MB, 4092 MB | own choice (estimated) |
| unpacking | default overwrite rule | ask | reference/09-extract-general.png |
| unpacking | default unpack mode | unpack everything | reference/09-extract-general.png |
| unpacking | set file times on unpack | True | reference/10-extract-advanced.png |
| unpacking | delete archive after unpack | never | reference/10-extract-advanced.png |
| unpacking | ZIP time tolerance for 'newer' | 2 s | ZIP stores two-second times (format) |
| viewer | preview size limit | 64 MB | estimated |
| viewer | hex dump limit | 64 KB | estimated |
| system | threads default | CPU cores | reference/24-settings-system.png shows a per-machine count |
| history | recent archives kept | 12 | estimated |
| window | window size | 1000x640 px | estimated |
| window | minimum window size | 640x400 px | estimated |
| window | toolbar buttons | Add, Extract, Test, View, Delete, Search, Guide, Info | Repair (+ Note for ZIP) | reference/01-main-window.png, reference/31-archive-open.png (own wording) |
| window | toolbar icon size large/small | 30 / 20 px | estimated |
| window | list columns on disk | Name, Size, Packed, Type, Modified | request + reference/01-main-window.png |
| window | extra column in archives | Checksum (CRC32) | reference/31-archive-open.png |
| window | list row height | 22 px | estimated |
| window | status note shown for | 8 s | estimated |
| controls | double-click opens | True | reference/27-settings-file-list.png |
| controls | open archive | Ctrl+O | own choice (estimated) |
| controls | add | Ctrl+Shift+A | own choice (estimated) |
| controls | extract | Ctrl+E | own choice (estimated) |
| controls | extract here | Ctrl+Shift+E | own choice (estimated) |
| controls | test | Ctrl+T | own choice (estimated) |
| controls | view | F3 | own choice (estimated) |
| controls | delete | Delete | own choice (estimated) |
| controls | info | Ctrl+I | own choice (estimated) |
| controls | search | Ctrl+F | own choice (estimated) |
| controls | up one level | Backspace | own choice (estimated) |
| controls | select by pattern / unselect | + / - | own choice (estimated) |
| checks | screenshot after | 120 frames | Craft Anything instructions |
