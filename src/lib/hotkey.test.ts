import { formatHotkey } from "./hotkey";

const key = (code: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) => ({
  code,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe("formatHotkey", () => {
  it("formats modifiers in a stable order with letter and digit codes simplified", () => {
    expect(formatHotkey(key("KeyM", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+M");
    expect(formatHotkey(key("Digit1", { altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+1");
    expect(formatHotkey(key("KeyK", { metaKey: true }))).toBe("Super+K");
  });

  it("allows bare function keys but not bare letters", () => {
    expect(formatHotkey(key("F13"))).toBe("F13");
    expect(formatHotkey(key("KeyM"))).toBeNull();
  });

  it("ignores lone modifier presses", () => {
    expect(formatHotkey(key("ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(formatHotkey(key("ControlRight", { ctrlKey: true }))).toBeNull();
  });
});
