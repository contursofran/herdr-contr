# 108: hover controls on an agent row, and the model tooltip

- Slice 3 of the sidebar rework (#98). On hover an agent row shows Stop (`terminal.kill`) and "Open in a new
  tab" (`terminal.keep` with `place: "tab"`): the commands that Herdr's own mods use.
- The hover state is the hovered address in the view, from `on_hover`, as in Herdr's `agents` mod. The runtime
  has no group-hover that hides and shows children.
- The model shows as Herdr's tooltip (`.tooltip(text)`) on agent and worker rows. A mod cannot set its look.
- The model comes from a small copy of the helper call: a mod cannot import another mod, and herdr-contr does
  not publish the model in shared state. It is asked from a timer, never from a draw.
- The agents copy (`herdr-contr-agents`) goes. The install removes an installed copy that nobody changed
  (`RETIRED` in `install.rs`).
