<div align="center">

<img src="https://github.com/AniXeno/eonos/blob/main/assets/branding/title_white.png?raw=true" alt="EonOS" width="500">

</div>

# EonOS

[![Language: Rust](https://img.shields.io/badge/Language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Bootloader: Limine](https://img.shields.io/badge/Bootloader-Limine-blue.svg)](https://limine-bootloader.org/)
[![Architecture: x86_64](https://img.shields.io/badge/Architecture-x86__64-lightgrey.svg)](https://en.wikipedia.org/wiki/X86-64)

A custom x86_64 hobby operating system kernel written in Rust and booted using the Limine boot protocol.

EonOS is built from the ground up as an experimental modern operating system project. The main goal is to explore low level systems programming, kernel architecture, memory management, hardware interaction, and the fundamentals required to build an operating system.

The project is still in early development and is primarily intended for learning, experimentation, and eventually building a more complete standalone operating system.

## Project layout

- `src/arch/`, `src/memory/`, `src/filesystem/`, and `src/task/` contain the
  kernel subsystems grouped by responsibility.
- `src/io/` contains console, framebuffer, serial, logging, and block I/O code;
  hardware drivers live in `src/drivers/`.
- `config/` contains the Limine boot configuration, linker script, and target
  specifications.
- `assets/` contains fonts and project branding. `initramfs/` contains files
  bundled into the root filesystem, while `userland/` contains the initial
  userspace programs.
- `vendor/doomgeneric/` pins the upstream Doom engine; `userland/doom/` holds
  the EonOS platform hooks and freestanding C runtime.
- `tests/` contains the QEMU smoke test, and `tools/` contains asset utilities.
- Build and launch entry points (`build.sh`, `run.sh`, and `runb.sh`) remain at
  the repository root.

The default userspace shell is the Rust program bundled as `/init`; its prompt
uses the `superuser@eonos:/path#` format. The former assembly shell remains
available as `/asm-shell`. The Rust shell can launch the bundled `/hello` ELF
with `exec /hello`.
Rust user programs can be written as `no_std` binaries using the small runtime
in `userland/runtime/`; the build includes `/rust-hello` as an example. Run it
from the shell with `exec /rust-hello`. The runtime provides process startup
and arguments, basic syscalls, formatted console output, panic reporting, and
a slab-backed allocator for small Rust `alloc` requests, with page allocations
for larger requests. Rust `std` and a libc compatibility layer are not
available as a general system library; DOOM has a small freestanding C shim
that delegates allocation, file I/O, and process exit to the Rust runtime.
The `userland/lib/` crate adds safe file and child-process helpers on top of
that runtime. Try `exec /rust-file-demo` after mounting `/disk` to exercise
create/write/read, or `exec /rust-process-demo` to spawn `/hello` and collect
its exit status. These small programs are templates for future Rust userland
tools.
`eonos_user::read_line(0)` provides bounded UTF-8 terminal line input for
interactive programs; `exec /rust-input-demo` exercises it.
The Rust shell supports quoted arguments, command and path tab completion,
history, per-process working directories (`cd`, `pwd`, relative paths), file
operations, process jobs (`exec ... &`, `jobs`, `wait`, `kill`), mounting, and
storage commands. Inspection built-ins include `head [-n lines] <file>`,
`wc <file>...`, `tree [directory]`, `which <command>`, and `free` for physical
memory and kernel heap usage. `tree` limits traversal to eight levels and 256
entries. Standalone Rust tools in `/bin` include `head`, `tail`, `grep`, and
`wc`; these read from standard input when no file is supplied, so they work in
pipelines such as `cat /etc/motd | grep EonOS`.
Commands without a slash are searched through the colon-separated `PATH` variable;
`export PATH=/bin:/disk/bin` changes the search directories for the current
shell and its child processes. Environment variables can be listed with `env`,
set with `export NAME=value`, removed with `unset NAME`, and expanded as `$NAME`
or `${NAME}`. Single-quoted text is not expanded.

At startup, the shell reads `/etc/profile`. The root initramfs is read-only, so
edit `initramfs/etc/profile` on the host and rebuild to set startup defaults.
`source <file>` (also available as `run <file>`) executes a text file one line
at a time in the current shell, so its `export` and `cd` changes remain active.
Script files can use blank lines and `#` comments; nested scripts are limited
to 16 levels. Try `source /examples/hello.sh` for a bundled example.

The shell supports pipelines and standard input/output redirection. For example,
`echo hello | cat`, `cat < /disk/notes.txt`, `echo hello > /disk/notes.txt`, and
`echo again >> /disk/notes.txt`. `>` creates or truncates a file, while `>>`
appends. Ctrl-C interrupts a foreground child and returns to the shell.
Diagnostic commands include `mem` for byte-level allocatable physical-memory
and kernel-heap counters, `mounts` for active mountpoints and their sources,
`devinfo` for block, network, and USB mouse status, and `kdiag` for a combined
snapshot of uptime, current process, memory, process listings, storage devices,
mounts, and E1000 counters. Existing focused commands such as `ps`, `free`,
`lsblk`, `net`, and `mouse` remain available.

Use `./runb.sh` for an optimized release build, which suppresses `DEBUG` log
lines and creates `build/iso_output/eonos.iso`. Use
`EONOS_PROFILE=dev ./runb.sh` to run an unoptimized development build. Use
`./runb.sh --debug` to include `DEBUG` logs and create
`build/iso_output/eonos_debugbuild.iso` (development profile by default).
`run.sh` uses the release profile by default and records the profile alongside
each ISO so it rebuilds when the requested profile changes.
`run.sh` prefers QEMU's standalone `virtio-gpu-pci` device, then tries
`virtio-vga`, and falls back to standard VGA. It performs a host-only startup
probe before selecting either VirtIO model. It requests a 1360×765 scanout for
VirtIO and standard VGA, and the VirtIO driver sets its scanout to that size
instead of inheriting QEMU's initial 640×480 mode. This is 85% of the previous
1600×900 dimensions, with GTK zoom-to-fit still off. Limine requests the same
resolution for the guest framebuffer. The script disables QEMU's implicit VGA
adapter for the standalone VirtIO device and keeps GTK zoom-to-fit off for 1:1
pixel scaling. Set
`EONOS_QEMU_VIDEO=std`,
`EONOS_QEMU_VIDEO=virtio-gpu-pci`, or `EONOS_QEMU_VIDEO=virtio-vga` to select
one explicitly. On Arch Linux, install the standalone PCI model and its shared
backend with
`sudo pacman -Syu qemu-hw-display-virtio-gpu
qemu-hw-display-virtio-gpu-pci`. The backend package is required by both the
PCI and VGA frontends; installing only a frontend can make QEMU crash while
creating the device.

Run `python3 tests/qemu_smoke.py` for a headless QEMU smoke test. It builds
into a temporary directory, checks boot and shell behavior, then rebuilds and
reboots with the same disk image to verify file persistence. It triggers the
`panic` command last to verify the kernel panic path. Prerequisites are QEMU,
the Rust nightly toolchain, GNU binutils, `dosfstools`,
`mtools`, and `xorriso`.

The initramfs is mounted as `/`. The build creates a persistent, sparse 4 GiB
FAT32 image at `build/eonos_disk.img` and attaches it to QEMU as an NVMe disk.
The image is preserved across builds; files written to it survive guest
reboots. The disk starts unmounted so writes require an explicit mount, for
example `mount /device/nvme0n1 /disk`. `/device/...` is the device namespace
used by EonOS for devices such as `/device/console`. `lsblk` lists detected
disks and filesystems, including their mountpoints. FAT32 supports short and
long names, file writes, directory creation, and removal. Shell commands include
`mkdir`, `rm`, `touch`, `cp`, and `write`; for example, `mkdir /disk/notes` followed by
`write /disk/notes/todo.txt remember`. `edit /disk/notes/todo.txt` opens a
small line-oriented editor: enter lines to append, `.save` to write, or
`.quit` to leave. `cp source destination` copies files in 4 KiB chunks.
User processes receive stdin, stdout, and stderr as descriptors 0, 1, and 2.
`/device/console` opens canonical console input and output through the
file descriptor API. `/device/null`, `/device/zero`, and `/device/urandom`
provide EOF/discard, zero-filled, and random-byte streams. `urandom` uses
RDRAND when available and a timing-seeded fallback otherwise; it is not
intended for cryptographic secrets. `exec`
launches a child ELF with `argc`, `argv`, and
`envp` on its initial stack; `wait4` only reaps children of the caller.
Opened files use per-process descriptors with sequential reads and writes;
`open` supports read-only, write-only, read/write, create, truncate, and append
modes.

The shell's `sync` command flushes mounted storage to the backing disk. The
`reboot` command flushes storage and restarts the machine; `shutdown` (also
`poweroff`) flushes storage and requests ACPI soft-off. FAT32
mutations also issue a device flush before reporting success. The root
initramfs remains read-only. After mounting the disk, `ls` shows `/disk` in the
root listing; use `cat /disk/README.TXT` or `exec /disk/HELLO.ELF` to access
the bundled files.
Era requires a writable filesystem mounted exactly at `/disk`; the shell and
kernel both reject attempts to launch it without that mount. This keeps its
settings and installed applications on persistent storage.

## Networking

EonOS currently has a polling Intel E1000 Ethernet driver for common
82540EM, 82545EM, and 82574L PCI devices. It initializes link state and the
MAC address and provides raw Ethernet frame transmit/receive to user programs.
The kernel network layer adds Ethernet II framing, ARP replies for its
configured address, IPv4 packet construction/parsing, UDP datagrams, and a
bounded DHCP discover/request client. Run `net dhcp` to request an IPv4 lease;
`net` shows link, MAC, counters, and the current address. `net recv` remains
available for raw frames. Use `net udp-send <ip> <port> <source-port> <text...>`
to send a datagram and `net udp-recv [max-bytes]` to poll for one. Rust programs
can use `eonos_rt::udp_send` and `eonos_rt::udp_receive`. DHCP supplies the DNS
server address; `nslookup <hostname>` performs a DNS A lookup, and
`ping <hostname> [count]` resolves names before sending ICMP echo requests.
The polling TCP client supports one active IPv4 connection at a time. `curler`
downloads HTTP and HTTPS files, including raw GitHub files. For example,
`curler https://raw.githubusercontent.com/OWNER/REPOSITORY/BRANCH/path/app.eonapp /disk/app.eonapp`
downloads an app package, which can then be installed with
`install /disk/app.eonapp`. It accepts Content-Length and chunked response
bodies and verifies HTTPS certificates against the bundled WebPKI root set.
The kernel supplies TLS randomness through the hardware RDRAND instruction and
uses the CMOS RTC time for certificate validity checks, so TLS fails safely if
the CPU has no working RDRAND or the RTC has no valid date. Mount the writable
disk at `/disk` first to keep downloads across reboots. `curler` follows up to
five HTTP redirects and refuses redirects that downgrade HTTPS to HTTP. Multiple
simultaneous TCP connections are not supported yet. At boot the kernel
reads the CMOS real-time clock, treats it as UTC, and advances that reading from
the monotonic PIT timer. `eonos_rt::wall_time_secs()` returns Unix time for
certificate validation and user programs; it returns `None` when no valid RTC
date is available. Era's taskbar clock shows UTC time and displays `--:--` when
the RTC could not be read. QEMU is launched with its RTC in UTC mode.
The QEMU launch scripts attach an E1000 adapter to an isolated user-mode
network. TCP, UDP, and DNS are polling implementations; DNS supports A records
only, DHCP lease renewal and full ARP neighbor management are not implemented.

The xHCI driver accepts directly connected USB HID boot keyboards and mice.
USB arrow keys feed the same line-editor sequences as PS/2, so command history,
cursor movement, and Tab completion work in the regular shell as well as Konsul.
The QEMU launcher attaches both; use the shell's `mouse` command to inspect
pointer coordinates and button state. Rust programs can poll the same state
with `eonos_rt::mouse_state()`.

Run `era` from the shell to open the Era graphical environment. Its desktop has
a clickable welcome window, a navigable Files app with an 8 KiB text preview, a
System Monitor with General, Network, and OS tabs, an Apps directory with
command examples, and a dock that opens the Rust
shell. Konsul supports USB keyboard command history with Up/Down and command
and path completion with Tab. The top-right button returns to the terminal. Era draws into a RAM
back buffer and presents changed regions; mouse movement updates only the small
cursor area. Rust programs can use `eonos_rt::era` for clipped rectangles,
PSF-font text, and explicit presentation. General Rust programs can use
`eonos_rt::graphics::Surface` for safe framebuffer drawing without receiving a
framebuffer pointer. It adds clipped pixels, rectangles, lines, circles, PSF
text, packed `0xRRGGBB` image blits, and explicit presentation; the existing
Era calls remain available for compatibility. Run `fb-test` in the shell to
show a rectangle test pattern, clear the surface, and return to the terminal.

## DOOM

EonOS includes the upstream `doomgeneric` engine at the commit recorded in
`vendor/doomgeneric/UPSTREAM_REVISION`. The engine remains C; `userland/doom/`
provides its EonOS platform hooks and a freestanding libc subset. The user ELF
uses the x86-64 System V C ABI and links statically without SDL, X11, or host
libraries. GCC is used by the Cargo build script (`EONOS_CC` can select another
x86-64 GCC-compatible compiler). The kernel enables FXSAVE/SSE2 and saves the
per-thread floating-point/SSE state during preemption.

`assets/doom/freedoom1.wad` is the Phase 1 IWAD from Freedoom 0.13.0; its
license and credits are included alongside it. This game data is compatible
with original Doom and Ultimate Doom. The build places the IWAD at
`/doom/freedoom1.wad` in the read-only initramfs.

Build with `./build.sh release` for normal gameplay performance (`./build.sh`
uses an unoptimized debug kernel), then start Era with `era` from the shell.
Open **Apps** and select **DOOM**; the same app can be launched from the
shell with `exec /bin/doom`. It runs as a normal isolated user process and
takes the existing exclusive graphics surface full screen. The 320×200 game
frame is scaled to the display while preserving its aspect ratio. Arrow keys
move and turn; Ctrl fires, Space uses, Shift runs, Tab opens the map, and Esc
opens the game menu. Press Ctrl+Q to exit to Era. Keyboard focus and the
graphics surface are released when the process exits, including on errors.

This initial port is software-rendered and has no audio or mouse support.
The VirtIO GPU path provides 2D framebuffer scanout and dirty-rectangle
updates; DOOM's renderer still runs on the CPU, so this improves presentation
and copy behavior rather than adding 3D acceleration.
Freedoom is bundled for immediate play; config and savegame writes are not
persistent because the root initramfs is read-only. `doom -iwad /path/to/game.wad`
can select another IWAD; missing and malformed files exit with a readable error.
`python3 tests/qemu_smoke.py` builds and boots QEMU, launches DOOM from Era,
checks its first presented frame, Ctrl+Q exit and relaunch, and exercises the
missing/malformed IWAD errors. Use a visible QEMU display for a manual check of
movement, firing, and sustained gameplay.

The kernel probes AHCI/SATA and NVMe controllers for block access. NVMe
devices support writes; AHCI/SATA remains read-only. To use a different raw
NVMe disk image in QEMU, set `EONOS_NVME_DISK=/path/to/disk.img ./run.sh`.

## Writing Rust applications for EonOS

Third-party programs are freestanding Rust ELF applications. They use the
`eonos-app` SDK, which builds against the same x86-64 EonOS user ABI as the
system programs. Every package declares `type = "SHELL"` or `type = "ERA"` in
`Cargo.toml` and repeats that type in `eonos_app::app!(SHELL, main)` or
`eonos_app::app!(ERA, main)`. The external
[`The-EonOS-Project/eon-apps`](https://github.com/The-EonOS-Project/eon-apps)
repository contains the app builder and projects. Check it out as `eon-apps/`
next to this repository. `eon-apps/app.sh build <app-directory>` checks that
the manifest and embedded ELF marker agree, then creates a portable
`.eonapp` package containing the description, type, id, and ELF executable.
Copy the package onto the data disk and install it at runtime.

```sh
(cd eon-apps && ./app.sh new hello-shell SHELL)
(cd eon-apps && ./app.sh build hello-shell)
EONOS_BUILD_DIR=build eon-apps/app.sh build eon-apps/hello-shell
./build.sh release
```

To bundle an app into the ISO, build it with `EONOS_BUILD_DIR=build` before
`./build.sh release`; the ISO includes packages from `build/apps` in
`/packages`. Mount the disk in EonOS, then install a bundled package with
`install /packages/hello-shell.eonapp`. Packages copied onto the disk can be
installed from `/disk/<id>.eonapp` instead. The installer validates its metadata and
ELF marker and writes the app to `/disk/apps/bin` with its manifest under
`/disk/apps/manifests`. Installed apps survive rebuilds and reboots; Era scans
the persistent registry at startup and shows them in its Applications window.
Use `uninstall <app-id>` to remove one. `SHELL`
apps run as commands and, when selected in Era, open in Konsul with their output
shown there. They can also be invoked from a normal EonOS shell. `ERA` apps
launch only from Era; the shell reports that they must be opened from the Apps
window. Each app has its own process and address space. A shell app selected in
Era runs as a supervised child of Konsul, and an Era app runs as a supervised
child connected to its Era window session.

The SDK exposes `eonos_app::system` for stdout, time, sleeping, memory status,
directory listing, and bounded read-only file access. For graphics, an ERA app
uses `eonos_app::era::Window`: `open`, `fill_rect`, `text`, `present`, and
`poll_event`. It also provides `button(x, y, width, height, label)`, which
draws a standard button and returns true when clicked, and
`dropdown(x, y, width, height, options, &mut selected)`, which draws a
selectable list and returns true when the selection changes. Form controls
also include `text_input(x, y, width, height, &mut value, placeholder)` for
single-line editing and `checkbox(x, y, label, &mut checked)` for toggles.
Redraw widgets each frame, polling events before drawing and calling
`present()` afterward. Text input supports printable keys, Backspace/Delete,
Left/Right, and Home/End, with a 256-byte value limit.
Tab and Shift+Tab move the visible focus between widgets in draw order. Enter
or Space activates a focused button or opens/closes a focused dropdown; arrow
keys change the focused dropdown's selection. Focus is shown with a blue
outline.
For example, `window.button(24, 24, 140, 36, "Continue")?` can be used as an
`if` condition. Its drawing commands and input events travel over pipes to Era;
the app never receives a framebuffer address. Era validates packet sizes,
clips drawing to the app window, and routes keyboard and pointer events only to
the active app. The event key value is a USB HID usage code. Escape is
available for an app's own close behavior, and the window close button
terminates it.

The kernel enforces the application sandbox even when a program bypasses the
SDK. Executables under `/apps/bin`, `/disk/apps/bin`, and their descendants
cannot mount or sync storage, control power, seize physical keyboard focus, use
raw framebuffer graphics, open raw network sockets, or request a kernel panic.
The SDK provides a bounded HTTP(S) GET operation through a fixed system helper:
TLS certificates are verified, redirects are limited, and response bodies are
capped at 256 KiB. Apps receive response bytes rather than raw network access.
SDK file operations are restricted to the mounted `/disk`; ordinary
process-local memory, sleeping, and assigned standard pipes remain available.
Audio, shared-memory surfaces, and an
independently managed app lifecycle service are not provided. Era apps use a
single bounded drawing-command stream rather than a GPU-backed surface; shell
apps get terminal output through Konsul.
