// Floating-panel sizing for the palette window. Kept pure (no Tauri/DOM) so the
// content-fit math is unit-testable; the component wires it to a ResizeObserver.
//
// The palette window is a fixed-width transparent float. Sizing its *height* to
// the rendered panel removes the dead transparent space that would otherwise
// reveal the main window behind the palette as a phantom "second layer".

/// Window inner width, in logical px. Must match `PALETTE_WIDTH` in
/// `crates/agentic-hub/src/palette.rs`.
export const PALETTE_WIDTH = 680;

/// Transparent frame kept on every side of the panel so its shadow falls off
/// before the window edge. Matches the outer `p-3` padding in CommandPalette.
export const PALETTE_FRAME = 12;

/// Window inner height that exactly fits the panel plus its frame.
export function paletteWindowHeight(panelHeight: number): number {
  return Math.ceil(panelHeight) + PALETTE_FRAME * 2;
}
