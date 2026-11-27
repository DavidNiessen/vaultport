---
title: Workspaces & broadcast
description: Group several sessions in one tab as a split grid or in focus mode, save them as templates and broadcast keystrokes to all terminals.
---

# Workspaces & broadcast

A **workspace** puts several terminal sessions into one tab.

## Creating a workspace

- In the Hosts view, select several hosts (<kbd>Mod</kbd>/<kbd>Shift</kbd>+click) and choose **Create workspace**.
- The new tab is named after the selection, for example _Workspace · 4 hosts_.

## Layouts

- **Split view** shows all sessions side by side in a grid.
- **Focus** shows one session large; switch between sessions from the workspace bar.

Toggle between them with <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd>.

## Broadcast

Turn on **Broadcast** (<kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd>) to send every keystroke to all terminals in the workspace — handy for running the same update on a fleet, or comparing output side by side. The toggle is highlighted while broadcast is on; turn it off before typing anything host-specific.

::: warning
Broadcast sends exactly what you type, including mistakes, to every host at once. Snippets can also [run on many hosts](/guide/snippets#running-on-several-hosts) with a confirmation step instead.
:::

## Saving workspaces

**Save** stores the workspace — its hosts and layout — in the vault. Saved workspaces appear under **Workspaces** in the new-tab dialog (<kbd>Mod</kbd>+<kbd>T</kbd>), so you can reopen the same set of sessions with one click, and they sync with the rest of your vault.

## Tips

- Give the hosts of a workspace distinct terminal themes per environment to see at a glance which pane is production.
