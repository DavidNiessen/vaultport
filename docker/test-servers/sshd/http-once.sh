#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Answers one HTTP request on stdin/stdout with /srv/www/index.txt (used through socat).
read -r _request_line
printf 'HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n'
cat /srv/www/index.txt
