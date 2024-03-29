# virterm

_virterm_ is a virtual terminal that executes a process in an off-screen
terminal. You can control the process and the terminal with a lua script. It
can be useful to make screenshots of or testing a cli/tui apps.

Supports Linux, Macos, Windows.

Features:

- make png screenshot
- make text screenshot
- wait until desired text appears in terminal
- send keys
- send mouse events
- send signal (unix)
- resize running terminal

## Table of contents

<!--ts-->

- [Usage](#usage)
  - [Lua api](#lua-api)
    - [vt.start(command [, params]) -&gt; proc](#vtstartcommand--params---proc)
    - [vt.sleep(duration_ms: int)](#vtsleepduration_ms-int)
    - [proc:pid() -&gt; int](#procpid---int)
    - [proc:cell(opts: table) -&gt; table](#proccellopts-table---table)
    - [proc:contents() -&gt; string](#proccontents---string)
    - [proc:send_str(str: string)](#procsend_strstr-string)
    - [proc:send_key(key: string)](#procsend_keykey-string)
    - [proc:click(opts: table)](#procclickopts-table)
    - [proc:scroll(opts: table)](#procscrollopts-table)
    - [proc:send_signal(signal: int | string)](#procsend_signalsignal-int--string)
    - [proc:kill()](#prockill)
    - [proc:resize(size: table)](#procresizesize-table)
    - [proc:wait()](#procwait)
    - [proc:wait_text(text:string [, opts])](#procwait_texttextstring--opts)
    - [proc:dump_txt(path: string)](#procdump_txtpath-string)
    - [proc:dump_png(path: string)](#procdump_pngpath-string)

<!-- Created by https://github.com/ekalinin/github-markdown-toc -->
<!-- Added by: pvolok, at: Sat Aug 20 00:01:13 CEST 2022 -->

<!--te-->

## Usage

Run `virterm my-script.lua`

Example lua script:

```lua
local proc = vt.start("nvim", { width = 120, height = 20 })

print("Pid: " .. proc:pid())

-- Wait until the terminal screen contains "[No Name]" text.
proc:wait_text("[No Name]")

print(proc:contents())

proc:send_str("iHello")
proc:send_key("<Enter>")
proc:send_str("World")

proc:wait_text("World")

proc:resize({ width = 60, height = 30 })
vt.sleep(300)

proc:dump_png("screenshot.png")

proc:send_signal("SIGTERM")
proc:wait()
```

### Lua api

#### `vt.start(command [, params]) -> proc`

Starts a new process.

- **command** - Shell command to run. Example: `"vim file.txt"`.
- **params** - Table with parameters
  - **height** - _Optional_. Terminal height in rows. Default: `30`.
  - **width** - _Optional_. Terminal width in columns. Default: `80`.

#### `vt.sleep(duration_ms: int)`

Sleeps for `duration_ms` milliseconds.

#### `proc:pid() -> int`

Returns process' pid.

#### `proc:cell(opts: table) -> table`

Get info about one cell on the terminal screen.

- **opts**
  - **x** - column (starts from 0).
  - **y** - row (starts from 0).

Returns:

- **content** - Cell content as a string.
- **fg** - Cell foreground color if defined. Can be a number for
  index colors or a string for rgb colors.
- **bg** - Cell background color if defined. Can be a number for
  index colors or a string for rgb colors.
- **bold**
- **italic**
- **underline**
- **inverse**
- **wide** - True if the cell content takes more than one character
  width.

#### `proc:contents() -> string`

Returns terminal screen content as a string.

#### `proc:send_str(str: string)`

Sends a string to stdin of the process.

#### `proc:send_key(key: string)`

Sends a key as an input to the process (into stdin).

Key examples:

- `<a>` "a" key
- `<C-a>` Control-a
- `<S-a>` Shift-a
- `<M-a>` Alt-a
- `<Enter>` Enter key
- `<Esc>` Escape key
- `<BS>` Backspace
- `<Left>`/`<Right>`/`<Up>`/`<Down>`

#### `proc:click(opts: table)`

Send mouse click event.

- **x** - column (starts from 0).
- **y** - row (starts from 0).
- **button** = _Optional_. Mouse button. Possible values: `"left"`, `"right"`,
  `"middle"`. Default is `"left"`.

#### `proc:scroll(opts: table)`

Send mouse scroll event.

- **x** - column (top line is 0).
- **y** - row (left row is 0).
- **dir** = Scroll direction. Possible values: `"up"`, `"down"`.

#### `proc:send_signal(signal: int | string)`

Send a signal to the process.

#### `proc:kill()`

Kill the process.

#### `proc:resize(size: table)`

Resize the terminal of the process.

- **height** - height in rows.
- **width** - width in columns.

#### `proc:wait()`

Wait until the process exits.

#### `proc:wait_text(text:string [, opts])`

Wait until the terminal contains provided text. The terminal is checked every
50 milliseconds. When _timeout_ expires, virterm exits with non-zero exit code.

- **opts**
  - **timeout** - _Optional_. Timeout in milliseconds. Default: `1000`.

#### `proc:dump_txt(path: string)`

Output terminal content as a text file.

#### `proc:dump_png(path: string)`

Renders and outputs terminal screen as a png file.
