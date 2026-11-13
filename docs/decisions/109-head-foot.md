# 109: the sidebar's head and foot

- The owner's design (2026-11-13, slice 2 of the sidebar rework, #97): the head has "Herdr", the machine
  switch, search and the sidebar toggle; the foot has the usage bars, Settings and an add machine button.
- The head is a view of `herdr-contr-spaces` (`head.js`), not of `herdr-contr`: the machine data (hc's list,
  the rebuilt and login states) is in that mod. The list writes the machines to a shared key; the head and its
  menu read it. The pick is shared too, so the head and the list agree.
- The Projects list shows the picked machine only. The machine fold is removed; "+" makes a space on the
  picked machine; a machine that is not connected has a line over its rows, with Connect or the login command.
- The foot is one block, a view of `herdr-contr`, in place of three (usage, the herdr-contr row, Herdr's
  footer). Its height follows the number of plan rows.
- The herdr-contr row is removed. Its dialog opens with a right-click on Settings and with ⌘⇧,.
