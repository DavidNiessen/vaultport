<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A scripted terminal: connection output appears line by line and commands are typed out.
 * Pure DOM text, no video. Pauses off-screen and in background tabs; people who prefer
 * reduced motion get the finished transcript without animation.
 */
import { onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';

interface Segment {
  text: string;
  tone?: 'ok' | 'muted' | 'accent' | 'cyan' | 'prompt' | 'warn' | 'strong';
}

type Step =
  | { kind: 'out'; segments: Segment[]; delay: number }
  | { kind: 'type'; command: string; delay: number };

interface Line {
  id: number;
  segments: Segment[];
}

const PROMPT: Segment[] = [
  { text: 'deploy@prod-eu-1', tone: 'ok' },
  { text: ':', tone: 'muted' },
  { text: '~', tone: 'accent' },
  { text: '$ ', tone: 'muted' },
];

const SCRIPT: Step[] = [
  {
    kind: 'out',
    delay: 350,
    segments: [
      { text: 'Connecting to ', tone: 'muted' },
      { text: 'deploy@prod-eu-1', tone: 'strong' },
      { text: ' via ', tone: 'muted' },
      { text: 'bastion.example.net', tone: 'strong' },
      { text: ' …', tone: 'muted' },
    ],
  },
  {
    kind: 'out',
    delay: 520,
    segments: [
      { text: '✓ ', tone: 'ok' },
      { text: 'host key   ', tone: 'muted' },
      { text: 'ED25519 SHA256:pQ3vX9…H1cE', tone: 'cyan' },
      { text: '  matches known hosts', tone: 'muted' },
    ],
  },
  {
    kind: 'out',
    delay: 420,
    segments: [
      { text: '✓ ', tone: 'ok' },
      { text: 'key exchange ', tone: 'muted' },
      { text: 'mlkem768x25519-sha256', tone: 'accent' },
      { text: '  post-quantum hybrid', tone: 'muted' },
    ],
  },
  {
    kind: 'out',
    delay: 300,
    segments: [
      { text: '✓ ', tone: 'ok' },
      { text: 'cipher     ', tone: 'muted' },
      { text: 'chacha20-poly1305@openssh.com', tone: 'cyan' },
    ],
  },
  {
    kind: 'out',
    delay: 680,
    segments: [
      { text: '✓ ', tone: 'ok' },
      { text: 'auth       ', tone: 'muted' },
      { text: 'FIDO2 key “YubiKey”', tone: 'strong' },
      { text: '  touch confirmed', tone: 'muted' },
    ],
  },
  { kind: 'out', delay: 260, segments: [{ text: '' }] },
  { kind: 'type', delay: 500, command: 'uptime' },
  {
    kind: 'out',
    delay: 240,
    segments: [{ text: ' 14:02:11 up 41 days,  3:12,  1 user,  load average: 0.08, 0.11, 0.09' }],
  },
  { kind: 'type', delay: 700, command: 'sudo systemctl reload nginx' },
  { kind: 'type', delay: 900, command: '' },
];

const lines = ref<Line[]>([]);
const typing = shallowRef<{ text: string } | null>(null);
const root = ref<HTMLElement | null>(null);

let nextId = 0;
let disposed = false;
let visible = true;
let resume: (() => void) | null = null;
let observer: IntersectionObserver | null = null;

function onVisibility() {
  if (!document.hidden && visible) resume?.();
}

function waitUntilActive(): Promise<void> {
  if (visible && !document.hidden) return Promise.resolve();
  return new Promise((resolve) => (resume = resolve));
}

async function sleep(ms: number): Promise<void> {
  await new Promise((r) => window.setTimeout(r, ms));
  await waitUntilActive();
}

function pushLine(segments: Segment[]) {
  lines.value = [...lines.value, { id: nextId++, segments }];
}

function renderStatic() {
  lines.value = [];
  for (const step of SCRIPT) {
    if (step.kind === 'out') pushLine(step.segments);
    else if (step.command) pushLine([...PROMPT, { text: step.command }]);
  }
  typing.value = { text: '' };
}

async function play() {
  while (!disposed) {
    lines.value = [];
    typing.value = null;
    for (const step of SCRIPT) {
      if (disposed) return;
      await sleep(step.delay);
      if (step.kind === 'out') {
        pushLine(step.segments);
        continue;
      }
      typing.value = { text: '' };
      for (const char of step.command) {
        if (disposed) return;
        await sleep(45 + Math.random() * 70);
        typing.value = { text: typing.value.text + char };
      }
      if (!step.command) break;
      await sleep(260);
      pushLine([...PROMPT, { text: step.command }]);
      typing.value = null;
    }
    await sleep(5200);
  }
}

onMounted(() => {
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    renderStatic();
    return;
  }
  if (root.value && typeof IntersectionObserver !== 'undefined') {
    observer = new IntersectionObserver(([entry]) => {
      visible = entry?.isIntersecting ?? true;
      if (visible && !document.hidden) resume?.();
    });
    observer.observe(root.value);
  }
  document.addEventListener('visibilitychange', onVisibility);
  void play();
});

onBeforeUnmount(() => {
  disposed = true;
  observer?.disconnect();
  document.removeEventListener('visibilitychange', onVisibility);
  resume?.();
});
</script>

<template>
  <div ref="root" class="terminal" aria-hidden="true">
    <div v-for="line in lines" :key="line.id" class="line">
      <span v-for="(seg, i) in line.segments" :key="i" :class="seg.tone">{{ seg.text }}</span>
    </div>
    <div v-if="typing" class="line">
      <span v-for="(seg, i) in PROMPT" :key="i" :class="seg.tone">{{ seg.text }}</span>
      <span>{{ typing.text }}</span><span class="cursor" />
    </div>
  </div>
</template>

<style scoped>
.terminal {
  min-height: 100%;
  padding: 18px 20px 22px;
  color: #d8dcea;
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.7;
  white-space: pre;
  overflow: hidden;
}

.line {
  min-height: 1.7em;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ok {
  color: #5fd69b;
}
.muted {
  color: #7e86a3;
}
.accent {
  color: var(--sc-accent-text);
}
.cyan {
  color: #7be2ee;
}
.strong {
  color: #f4f6fb;
  font-weight: 500;
}
.warn {
  color: #f4c15c;
}

.cursor {
  display: inline-block;
  width: 0.6em;
  height: 1.15em;
  margin-left: 1px;
  vertical-align: -0.2em;
  background: #8b8ff8;
  animation: blink 1.1s steps(1) infinite;
}

@keyframes blink {
  50% {
    opacity: 0;
  }
}

@media (max-width: 640px) {
  .terminal {
    padding: 14px 14px 18px;
    font-size: 11px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .cursor {
    animation: none;
  }
}
</style>
