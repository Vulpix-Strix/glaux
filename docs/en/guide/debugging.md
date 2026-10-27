# Debugging a cartridge

The Code view can run a cartridge under the debugger. It stops at the lines you mark, shows the variables and the call stack there, and lets you go on one line at a time. It works for Luau and Rhai cartridges.

## Setting breakpoints

Press F9 to mark the line the caret is on, and again to clear it. Pressing a line number does the same for that line. A marked line shows its number on a red ground.

Breakpoints move with the text as you edit. Deleting a line deletes its breakpoint, and adding a line above one moves it down with its line. They stay when you close the script's tab.

A breakpoint on a line with no code stops on the next line that has some. In Rhai, a line that holds no statement, such as a closing brace, never stops.

## Starting and stopping

Press Ctrl+F5, or the bug plate beside the run plate, to run the cartridge under the debugger. Unsaved edits are written first, whatever Save Before Running says, because breakpoints are lines of the files.

The game runs normally until it reaches a breakpoint. Then the Code view comes back with the stopped line highlighted and an arrow beside it. Press Shift+F5 or Ctrl+. to end the run.

A cartridge under the debugger runs slower than one started with F5, so use it to look at a problem, not to play.

## While the cartridge is stopped

The scripts are read-only, and creating, renaming or deleting a script is blocked too, because the cartridge runs the files as they were when it started. You can still move the caret, select, search, open other scripts and use Go to Definition.

| Key      | Plate         | What it does                                              |
|----------|---------------|-----------------------------------------------------------|
| F5       | first plate   | goes on until the next breakpoint                         |
| F10      | second plate  | steps to the next line, without entering calls            |
| F7       | third plate   | steps to the next line, entering calls                    |
| Shift+F7 | fourth plate  | runs until the current function returns                   |
| Shift+F5 | stop plate    | ends the run                                              |

You can set and clear breakpoints while stopped. They apply when the cartridge goes on.

### Variables and the call stack

The panel on the right lists the variables of the selected frame. Drag its left edge to make it wider, up to half the view. It keeps the width you chose for the next stop. Press a row marked with a plus sign to open a table, and again to close it. Values are shown without running any of your code, so a table with a metamethod is shown as it is stored.

Below it, the call stack lists the frames from the innermost. Press a frame to move to its line and see its variables. A frame shown as the engine is not script code.

### Luau

Locals and the variables a function captures from the script around it are listed. Every frame has variables.

### Rhai

The state you keep in `this` is listed first, then the local variables. Only the innermost frame has variables.

## Limits

- The stop is inside a tick. The game does not draw while stopped, and the time the game sees does not advance.
- A script that loops without end is still stopped by its budget, and time spent stopped does not count against it.
- A Rhai loop written on one line stops once each time it is entered, not at every pass.
