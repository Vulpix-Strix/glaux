// A hand copy of ../signal-lost/code/sheet.luau, which tools/build_sheet.py
// generates there; keep the two in step when the sheet is rebuilt.
// Object-tile index of each sprite's top-left tile, with its size in tiles.

export const KNIGHT_IDLE = [0, 2, 4, 6]; // 2x3
export const KNIGHT_RUN = [8, 10, 12, 14]; // 2x3
export const ORC_RUN = [48, 50, 52, 54]; // 2x3
export const GOBLIN_RUN = [96, 98, 100, 102]; // 2x2
export const AXE = 128; // 2x2
export const HEART_FULL = 130; // 2x2
export const HEART_HALF = 132; // 2x2
export const HEART_EMPTY = 134; // 2x2
export const FLASK = 136; // 2x2
export const WALL_MID = 138; // 2x2
export const WALL_TOP = 140; // 2x2
export const FLOOR = [160, 162, 164, 166, 168, 170, 172, 174]; // 2x2
export const OGRE_RUN = [192, 196, 200, 204]; // 4x4
export const COIN = [256, 257, 258, 259]; // 1x1
export const SKULL = 260; // 1x1
