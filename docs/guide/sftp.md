---
title: SFTP
description: The dual-pane SFTP browser — local and remote panes, drag-and-drop transfers, a resumable queue, permissions and edit-and-upload.
---

# SFTP

The pinned **SFTP** tab is a dual-pane file manager. Each pane shows either **This device** or any saved host, so you can copy between your computer and a server — or between two servers.

## Browsing

1. Open the SFTP tab (or choose **Open in SFTP** on a host).
2. In each pane, **Choose a location**: this device or a host. The pane opens in the host's configured SFTP start path, or the home directory.
3. Navigate with double-click, **Back**/**Forward**, the path bar and **Refresh**. **Filter** the listing and toggle **hidden files**.

Per host or group you can set the **start path**, whether **hidden files** are shown, and whether **symlinks are followed**.

## File operations

Right-click (or use **Actions**) for **Open**, **Download**, **Upload**, **Rename**, **New folder**, **Delete** and **Permissions…** (octal, for example `644`). Select several entries with <kbd>Mod</kbd>/<kbd>Shift</kbd>+click.

Deleting a folder is recursive but **never follows symbolic links**, so a link to `/` cannot take your system with it.

## Transfers

Drag files between the panes — or select them and choose **Copy to other pane** — to queue a transfer. The transfer list shows the state and progress of every item; you can cancel a transfer, and **Clear finished** tidies the list.

- Transfers run in parallel (with a concurrency limit) and pipeline SFTP requests for speed.
- Data is written to a temporary `<name>.sshc-part` file and renamed when complete, so an interrupted transfer never leaves a truncated file under the real name.
- An interrupted transfer is **resumed** from where it stopped if the partial file still matches the source.
- Folders are copied recursively. Server-to-server copies are streamed through your device.
- File names received from a server are validated before they are used locally; entries such as `../../.bashrc` are dropped and reported, so a malicious server cannot write outside the target folder.

## Edit locally

**Edit locally** downloads a remote file into a private temporary folder (mode `0700`, files `0600`) and opens it with your default application. When you save, ssh-client notices the change and offers to upload it.

- The upload **refuses to overwrite** the remote file if it changed on the server since you opened it, so you do not clobber someone else's edit.
- Remote permissions are preserved, and the upload uses the same part-file-and-rename approach.
- The temporary copy is removed when the edit session ends.

Change detection is available in the desktop apps.

## Connections

SFTP panes and transfers reuse a pooled SSH connection per host — including its jump hosts, proxy and credentials — instead of logging in again for every operation.
