# 31: a wider chat column

The owner (2026-11-12) asked for a wider chat: the messages and the reply box.

- `READ_W` in `estimate.js` goes from 760 px to 1000 px. Below that, the column is the pane less 24 px gutters.
- All parts that follow the column take their width from `columnWidth()`: messages, bubbles (78%), tool rows,
  the working line, cards, the tags row, the slash list and the reply box. No other width changes.
- The row height estimate takes the width as an argument, so it stays in step. The measured tests at 760 px
  stay as they are: they check the estimate at a known width, not the cap.
- The settings dialog (`SETTINGS_W`, 760 px) is not the chat column and does not change.
