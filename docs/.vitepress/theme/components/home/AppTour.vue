<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A scroll-driven tour of the real app. The screenshot stage stays pinned while the chapters
 * scroll past; each chapter cross-fades to its screenshot and moves a pulsing hotspot onto the
 * part of the UI it talks about. On narrow screens every chapter shows its own screenshot.
 */
import { withBase } from 'vitepress';
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';

interface Hotspot {
  x: number;
  y: number;
  w: number;
  h: number;
  label: string;
}
interface Chapter {
  id: string;
  kicker: string;
  title: string;
  text: string;
  points: string[];
  image: string;
  alt: string;
  hotspot?: Hotspot;
  phones?: boolean;
  link: { text: string; href: string };
}

const CHAPTERS: Chapter[] = [
  {
    id: 'terminal',
    kicker: 'Terminal',
    title: 'A terminal that floats on light.',
    text: 'The default Pulse theme is see-through: your session sits on a slow, living glow, and so does the side panel with your snippets, history, themes and local AI.',
    points: [
      'WebGL rendering, truecolor, OSC 7/8/52/133',
      'Search, ligature fonts and themes',
      'Autocomplete that never phones home',
    ],
    image: 'terminal',
    alt: 'A terminal session in the see-through Pulse theme with the snippets side panel open',
    hotspot: { x: 77.6, y: 5, w: 22, h: 46, label: 'Side panel' },
    link: { text: 'Snippets & autocomplete', href: '/guide/snippets' },
  },
  {
    id: 'vault',
    kicker: 'Vault',
    title: 'Every host, in one encrypted vault.',
    text: 'Groups pass settings down to their hosts, tags and favourites keep things findable, and the omnibox connects to anything you type — ssh user@host included.',
    points: [
      'Per-setting inheritance',
      'Import from ~/.ssh/config, PuTTY, MobaXterm, SecureCRT',
      'AWS, DigitalOcean and Azure inventory',
    ],
    image: 'hosts',
    alt: 'The hosts view with groups, tagged host cards and the omnibox',
    hotspot: { x: 18.4, y: 17.2, w: 69.5, h: 6.4, label: 'Omnibox' },
    link: { text: 'Hosts & groups', href: '/guide/hosts-and-groups' },
  },
  {
    id: 'workspace',
    kicker: 'Workspaces',
    title: 'Four servers. One keystroke.',
    text: 'Split a workspace into panes and broadcast your input to every one of them. Save the layout and bring the whole set back tomorrow.',
    points: ['Split and focus modes', 'Broadcast to every pane', 'Saved workspace templates'],
    image: 'workspace',
    alt: 'A workspace with four terminals side by side and broadcast input turned on',
    hotspot: { x: 78.8, y: 4.6, w: 9, h: 5.6, label: 'Broadcast' },
    link: { text: 'Workspaces & broadcast', href: '/guide/workspaces' },
  },
  {
    id: 'agents',
    kicker: 'AI agents',
    title: 'AI agents, on a short leash.',
    text: 'Vaultport speaks MCP. Claude Code, Cursor, VS Code or Codex can work on your hosts — but every action waits for your approval, and they never see a credential.',
    points: [
      'Off by default',
      'Per-action approval, read-only mode',
      'Every call in the activity log',
    ],
    image: 'agent',
    alt: 'An approval dialog for a command that Claude Code wants to run on a host',
    hotspot: { x: 32, y: 33.2, w: 36.2, h: 33.6, label: 'Your call' },
    link: { text: 'AI agents (MCP)', href: '/guide/ai-agents' },
  },
  {
    id: 'mobile',
    kicker: 'Phones & tablets',
    title: 'The same vault, in your pocket.',
    text: 'A touch-first layout with a terminal key bar on iOS, iPadOS and Android. Pair a phone with a QR code and it syncs through your own storage.',
    points: [
      'Accessory key bar with Ctrl, Esc and arrows',
      'Biometric unlock',
      'QR-code device pairing',
    ],
    image: 'phone-hosts',
    alt: 'The hosts list and a terminal session on a phone',
    phones: true,
    link: { text: 'Phones & tablets', href: '/guide/mobile' },
  },
];

const active = ref(0);
const chapterEls = ref<HTMLElement[]>([]);
let observer: IntersectionObserver | null = null;

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) active.value = Number((e.target as HTMLElement).dataset.index);
      }
    },
    { rootMargin: '-45% 0px -45% 0px' },
  );
  chapterEls.value.forEach((el) => observer?.observe(el));
});
onBeforeUnmount(() => observer?.disconnect());

const hotspot = computed(() => CHAPTERS[active.value]?.hotspot);
const src = (name: string) => withBase(`/screens/${name}.webp`);
</script>

<template>
  <section id="tour" class="tour">
    <div class="head container">
      <p class="eyebrow" v-reveal>Take the tour</p>
      <h2 class="h-section" v-reveal="80">
        Built like a native app. <span class="serif grad-text">Because it is one.</span>
      </h2>
      <p class="lede" v-reveal="160">
        A Rust core does everything that touches a secret; a fast, keyboard-first interface does the
        rest. Here is the real thing.
      </p>
    </div>

    <div class="body container">
      <div class="chapters">
        <article
          v-for="(c, i) in CHAPTERS"
          :key="c.id"
          :ref="
            (el) => {
              if (el) chapterEls[i] = el as HTMLElement;
            }
          "
          :data-index="i"
          class="chapter"
          :class="{ on: i === active }"
        >
          <p class="kicker mono">
            <span>0{{ i + 1 }}</span> {{ c.kicker }}
          </p>
          <h3>{{ c.title }}</h3>
          <p class="text">{{ c.text }}</p>
          <ul>
            <li v-for="p in c.points" :key="p">{{ p }}</li>
          </ul>
          <a :href="withBase(c.link.href)" class="more">{{ c.link.text }} →</a>
          <!-- Inline screenshot for narrow screens -->
          <figure class="inline-shot">
            <div v-if="c.phones" class="phones-inline">
              <img
                :src="src('phone-hosts')"
                alt="The hosts list on a phone"
                loading="lazy"
                width="780"
                height="1688"
              />
              <img
                :src="src('phone-terminal')"
                alt="A terminal session on a phone"
                loading="lazy"
                width="780"
                height="1688"
              />
            </div>
            <img
              v-else
              :src="src(c.image)"
              :alt="c.alt"
              loading="lazy"
              width="2000"
              height="1250"
            />
          </figure>
        </article>
      </div>

      <div class="stage-col">
        <div class="stage">
          <div class="progress" aria-hidden="true">
            <span
              v-for="(c, i) in CHAPTERS"
              :key="c.id"
              :class="{ done: i < active, on: i === active }"
            />
          </div>
          <div class="screen">
            <template v-for="(c, i) in CHAPTERS" :key="c.id">
              <div v-if="c.phones" class="shot phones" :class="{ on: i === active }">
                <img :src="src('phone-hosts')" alt="" loading="lazy" width="780" height="1688" />
                <img :src="src('phone-terminal')" alt="" loading="lazy" width="780" height="1688" />
              </div>
              <img
                v-else
                class="shot"
                :class="{ on: i === active }"
                :src="src(c.image)"
                :alt="i === active ? c.alt : ''"
                loading="lazy"
                width="2000"
                height="1250"
              />
            </template>
            <div
              class="hotspot"
              :class="{ off: !hotspot }"
              :style="
                hotspot
                  ? {
                      left: `${hotspot.x}%`,
                      top: `${hotspot.y}%`,
                      width: `${hotspot.w}%`,
                      height: `${hotspot.h}%`,
                    }
                  : {}
              "
              aria-hidden="true"
            >
              <span class="hotspot-label mono">{{ hotspot?.label }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tour {
  position: relative;
  padding: 120px 0 40px;
}
.head {
  display: grid;
  justify-items: start;
  gap: 18px;
  max-width: var(--page-max);
}
.head .lede {
  max-width: 620px;
}
.body {
  display: grid;
  grid-template-columns: minmax(0, 0.8fr) minmax(0, 1.6fr);
  gap: clamp(32px, 5vw, 72px);
  margin-top: 40px;
}
.chapter {
  min-height: 78vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 40px 0;
  transition: opacity 0.6s var(--ease-out);
}
/* Inactive chapters are dimmed by colour, not opacity, so they keep 4.5:1 contrast. */
.chapter:not(.on) :is(h3, .text, li) {
  color: var(--text-3);
}
.chapter :is(h3, .text, li) {
  transition: color 0.5s var(--ease-out);
}
.kicker {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: var(--accent);
}
.kicker span {
  display: grid;
  place-items: center;
  width: 30px;
  height: 22px;
  border-radius: 999px;
  border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
  font-size: 12px;
}
.chapter h3 {
  margin-top: 16px;
  font-size: clamp(28px, 3vw, 40px);
  line-height: 1.08;
  letter-spacing: -0.035em;
  font-weight: 600;
  text-wrap: balance;
}
.text {
  margin-top: 14px;
  color: var(--text-2);
  font-size: 16.5px;
  line-height: 1.65;
}
.chapter ul {
  list-style: none;
  padding: 0;
  margin: 18px 0 0;
  display: grid;
  gap: 8px;
}
.chapter li {
  display: flex;
  gap: 10px;
  font-size: 14.5px;
  color: var(--text-2);
}
.chapter li::before {
  content: '';
  flex: none;
  width: 6px;
  height: 6px;
  margin-top: 9px;
  border-radius: 2px;
  background: var(--accent);
  transform: rotate(45deg);
}
.more {
  margin-top: 18px;
  padding: 4px 0;
  font-weight: 550;
  font-size: 14.5px;
  color: var(--accent);
  width: fit-content;
}
.more:hover {
  text-decoration: underline;
  text-underline-offset: 4px;
}
.inline-shot {
  display: none;
}

.stage-col {
  position: relative;
}
.stage {
  position: sticky;
  top: calc(50vh - min(26vw, 300px));
  display: grid;
  gap: 14px;
}
.progress {
  display: flex;
  gap: 6px;
}
.progress span {
  position: relative;
  flex: 1;
  height: 3px;
  border-radius: 3px;
  background: var(--surface-3);
  overflow: hidden;
}
.progress span::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, var(--accent), var(--accent-2));
  transform: scaleX(0);
  transform-origin: left;
  transition: transform 0.6s var(--ease-out);
}
.progress span.done::after,
.progress span.on::after {
  transform: scaleX(1);
}
.screen {
  position: relative;
  aspect-ratio: 16 / 10;
  border-radius: 18px;
  padding: 5px;
  background: linear-gradient(180deg, rgb(255 255 255 / 0.14), rgb(255 255 255 / 0.03));
  box-shadow:
    var(--shadow-lg),
    0 0 140px -40px rgb(var(--glow-1) / 0.6);
}
.shot {
  position: absolute;
  inset: 5px;
  width: calc(100% - 10px);
  height: calc(100% - 10px);
  object-fit: cover;
  border-radius: 13px;
  background: #0d0c12;
  opacity: 0;
  transform: scale(1.04);
  filter: blur(8px);
  transition:
    opacity 0.7s var(--ease-out),
    transform 0.9s var(--ease-out),
    filter 0.7s var(--ease-out);
}
.shot.on {
  opacity: 1;
  transform: none;
  filter: none;
}
.shot.phones {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 6%;
  background:
    radial-gradient(60% 70% at 20% 0%, rgb(var(--glow-1) / 0.5), transparent 70%),
    radial-gradient(50% 60% at 100% 100%, rgb(var(--glow-2) / 0.35), transparent 70%), #0d0c12;
}
.shot.phones img {
  height: 88%;
  width: auto;
  border-radius: 22px;
  border: 1px solid rgb(255 255 255 / 0.16);
  box-shadow: 0 30px 60px -20px rgb(0 0 0 / 0.8);
  transition: transform 0.9s var(--ease-out);
}
.shot.phones.on img:first-child {
  transform: rotate(-4deg) translateY(4px);
}
.shot.phones.on img:last-child {
  transform: rotate(4deg) translateY(-4px);
}
.hotspot {
  position: absolute;
  border-radius: 12px;
  border: 1.5px solid var(--accent);
  box-shadow:
    0 0 0 4000px rgb(4 3 10 / 0.28),
    0 0 30px rgb(var(--glow-1) / 0.6),
    inset 0 0 20px rgb(var(--glow-1) / 0.25);
  transition:
    left 0.8s var(--ease-out),
    top 0.8s var(--ease-out),
    width 0.8s var(--ease-out),
    height 0.8s var(--ease-out),
    opacity 0.4s;
  pointer-events: none;
  animation: pulse 2.4s var(--ease-in-out) infinite;
}
.hotspot.off {
  opacity: 0;
  left: 50%;
  top: 50%;
  width: 0;
  height: 0;
}
@keyframes pulse {
  50% {
    border-color: var(--accent-2);
    box-shadow:
      0 0 0 4000px rgb(4 3 10 / 0.28),
      0 0 46px rgb(var(--glow-2) / 0.7),
      inset 0 0 26px rgb(var(--glow-2) / 0.3);
  }
}
.screen {
  overflow: hidden;
}
.hotspot-label {
  position: absolute;
  left: 50%;
  top: calc(100% + 8px);
  transform: translateX(-50%);
  padding: 3px 9px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

@media (max-width: 960px) {
  .body {
    grid-template-columns: 1fr;
  }
  .stage-col {
    display: none;
  }
  .chapter {
    min-height: 0;
    opacity: 1;
    padding: 32px 0;
  }
  .inline-shot {
    display: block;
    margin-top: 24px;
  }
  .inline-shot img {
    width: 100%;
    height: auto;
    border-radius: 12px;
    border: 1px solid var(--border);
    box-shadow: var(--shadow-md);
  }
  .phones-inline {
    display: flex;
    gap: 12px;
    justify-content: center;
  }
  .phones-inline img {
    width: 44%;
    border-radius: 18px;
  }
}
</style>
