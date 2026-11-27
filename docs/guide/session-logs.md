---
title: Session logs
description: Encrypted, tamper-evident recordings of SSH, Telnet, serial and local sessions in asciicast v2 format, with search, replay, bookmarks and export.
---

# Session logs

Vaultport records the output of your terminal sessions — SSH, Telnet, serial and local — into **encrypted session logs**, so you can look up what a command printed yesterday or keep a record of maintenance work.

Session logging is on by default. Turn it off or on again with **Settings → Security → Record session logs**; the switch applies to sessions you open afterwards. Open **Logs** in the vault to browse them.

## What is recorded

- Terminal **output**, window resizes and markers, in the open [asciicast v2](https://docs.asciinema.org/manual/asciicast/v2/) format with timing.
- **Keyboard input is not recorded.** Input recording is off by default in the engine, because what you type can include secrets; even when enabled, input is never recorded while a password prompt is detected.

## Encryption and integrity

Each log is a single file sealed in chunks with **XChaCha20-Poly1305**, using a key derived from your vault key. The data is DEFLATE-compressed before encryption, and chunks are sealed every 64 KiB or two seconds, so at most a few seconds are lost if the app is closed abruptly.

Every chunk is bound to its log and its position, and a final chunk records the totals. Flipped bits, swapped, reordered, foreign or missing chunks and truncation are all detected when the log is read. A log whose session crashed before it finished can still be opened up to the last intact chunk.

## Retention

- The **most recent session per host** is kept; older ones are deleted when a new session ends.
- **Bookmark** a log (**Keep this log**), optionally with a comment, to keep it permanently.
- The total size of all logs is capped at 1 GB; the oldest unbookmarked logs go first.

Log metadata (host, time, size, device, bookmark) is stored in the vault. The log files themselves stay on the device that recorded them.

## Using logs

- **Search logs** across sessions, or **Search in log** inside one — matching ignores colour and other escape sequences.
- **Replay** a session with its original timing, and **pause** it at any point.
- **Export as .cast** writes a plain, unencrypted asciicast file that tools like `asciinema play` understand. Treat exported files as sensitive.

The **History** side panel and command history are separate from logs; see [Snippets & autocomplete](/guide/snippets#command-history).
