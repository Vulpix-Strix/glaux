# Getting started

This guide creates a cartridge, edits its script and runs it. It takes a few minutes and needs only a keyboard.

## Create a cartridge

Glaux starts in a shell. Create a cartridge with `new` and give it an id, which is one word without spaces:

```text
new my-game
```

The new cartridge is loaded right away. It starts as a small demo with the Glaux owl, which you can move. To write Rhai instead of Luau, add `--lang rhai`. Rhai support is experimental.

A cartridge is a folder with these parts that matter here:

| File                  | Purpose                                                  |
|-----------------------|----------------------------------------------------------|
| `cartridge.ron`       | The manifest: name, version, language and entry script.  |
| `code/main.luau`      | The entry script. A Rhai cartridge has `code/main.rhai`. |
| Other `code/` files   | Modules the entry script loads.                          |
| `sprites.bin`, `objattr.bin`, `obp.bin` | The owl sprite and its palettes. |

## Run it

Type `run`. The owl appears. Move it with the arrow keys and press Z to make it hoot.

Press Escape to pause. Escape again resumes. Enter on the pause screen returns to the shell, where `resume` continues the cartridge and `stop` ends it.

## Credits and licenses

Type `credits` in the shell to see who made Glaux, facts about the system, and the license of every library and resource inside it. F1 to F4 switch between the Credits, System, Libraries and Licenses pages, `?` lists the keys, and Escape goes back one level and finally returns to the shell.

## Read the documentation

Type `docs` in the shell to open the documentation browser without loading a cartridge. `docs spr` opens the page of `spr`, and `docs guide/getting-started` opens a page by its id. When a name could mean several pages, such as `index`, the browser opens its search instead, so you choose. Escape closes the search, then returns to the shell. The tabs and history you left open are still there the next time.

## Edit the script

With the shell showing, press Escape to open the editor on your cartridge. Replace the whole script with this minimal game, which moves a square and counts the times you press Z:

### Luau

```luau
local x, y = 156, 116
local hoots = 0

function tick()
  if btn(Button.Left) then x = x - 2 end
  if btn(Button.Right) then x = x + 2 end
  if btn(Button.Up) then y = y - 2 end
  if btn(Button.Down) then y = y + 2 end
  if btnp(Button.A) then hoots = hoots + 1 end
end

function draw()
  cls(0)
  rectfill(x, y, 8, 8, 7)
  print("Presses: " .. hoots, 8, 8, 15)
end
```

### Rhai

```rhai
fn boot() {
    this.x = 156;
    this.y = 116;
    this.presses = 0;
}

fn tick() {
    if btn(Button::Left) { this.x -= 2; }
    if btn(Button::Right) { this.x += 2; }
    if btn(Button::Up) { this.y -= 2; }
    if btn(Button::Down) { this.y += 2; }
    if btnp(Button::A) { this.presses += 1; }
}

fn draw() {
    cls(0);
    rectfill(this.x, this.y, 8, 8, 7);
    text(`Presses: ${this.presses}`, 8, 8, 15);
}
```

Press F5 or the play button above the script to run it from the editor. The game fills the screen, and Escape pauses it. On the pause screen Escape resumes, R starts it over and Enter stops it and returns to the editor, with your tabs, cursor and undo history as you left them. By default every unsaved edit is saved before a run. The Save before running setting in the editor's settings turns that off, and the cartridge then runs the files as they are on disk.

You can also save with Ctrl+S, press Escape to go back to the shell and type `run` again.

## Read the log

While a cartridge runs, F12 shows the log panel over the lower half of the screen and hides it again. The game keeps running and does not receive your keys while the panel is open. It lists what your scripts logged with `log` and what Glaux itself reports. Up and Down move through the messages, End follows the newest one, L hides the less severe levels and S switches between your messages and Glaux's. Escape closes the panel.

When a run ends with an error that names a line of your script, the editor opens on that line. Open the log panel there and press Enter on the error to come back to the line later. A script that logs more than a few hundred messages in one tick has the rest dropped, and the log says how many.

## How it works

Glaux calls `tick()` and then `draw()` 60 times a second. `tick()` reads the buttons and changes the position. `draw()` clears the screen and draws the square and the text from that position. Nothing else is needed, because the variables keep their values between ticks.

The screen is 320 by 240 pixels with `(0, 0)` at the top left. Colors are palette indices, and `0` and `7` above are two of them. Drawing is only allowed inside `draw()`.

## Where to go next

- [The game loop](glaux://guide/game-loop) explains the callbacks, state and the differences between Luau and Rhai.
- The [script API](glaux://api/index) lists every function.
- Try [spr](glaux://api/graphics/spr) to draw sprites and [tone](glaux://api/audio/tone) to make sound.
