/**
 * Unsent-prompt wipe for agent TUIs. One byte string, same on Windows ConPTY
 * and macOS/Unix PTY — no Cmd/Alt, no OS shortcuts.
 *
 * Order matters:
 * 1. Readline kill-line (Agy, Claude, bash/zsh, many Ink ports)
 * 2. Delete-word (agents that bind Ctrl+W but not Ctrl+U)
 * 3. Backspace burst (Cursor Agent and any "plain text field")
 * 4. Forward-delete if Ctrl+A parked the cursor at the start
 *
 * Never: Ctrl+C (interrupt), Ctrl+D (exit Cursor), Ctrl+L (wipe screen), Esc (menus).
 */
export const CLEAR_DRAFT =
  '\x01\x0b\x15' + '\x17'.repeat(48) + '\x7f'.repeat(1024) + '\x1b[3~'.repeat(256);
