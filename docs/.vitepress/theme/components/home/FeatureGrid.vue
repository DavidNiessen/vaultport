<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * Bento grid of capabilities. Every card carries a small live demo (typing terminals, packets
 * hopping through a jump host, a security key waiting for a touch…) and a glow that follows the
 * pointer along its border. Demos only animate while the grid is on screen.
 */
import {
  ArrowRight,
  Cpu,
  FileInput,
  Fingerprint,
  History,
  KeyRound,
  Laptop,
  Server,
  ShieldCheck,
  Sparkles,
  SquareTerminal,
  Waypoints,
} from '@lucide/vue';
import { withBase } from 'vitepress';
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { prefersReducedMotion, useInView } from '../../composables/motion';

const grid = ref<HTMLElement | null>(null);
const routeSvg = ref<SVGSVGElement | null>(null);
const inView = useInView(grid);

/* Protocol terminal: types one command after another */
const COMMANDS = [
  'ssh deploy@api-prod-eu-1',
  'mosh pi@raspberry-pi',
  'sftp nas:/volume1/backups',
  'telnet 192.168.1.1 23',
  'serial /dev/ttyUSB0 115200',
];
const typed = ref(COMMANDS[0]!);
const history = ref<string[]>([]);

/* Snippet variable cycling */
const SERVICES = ['nginx', 'postgresql', 'redis', 'caddy'];
const service = ref(0);

/* Local AI: prompt -> command */
const aiCommand = ref('du -ah /var/log | sort -rh | head -5');
const aiShown = ref(aiCommand.value);

let timers: number[] = [];
let running = false;
function later(fn: () => void, ms: number) {
  timers.push(window.setTimeout(fn, ms));
}
function stop() {
  timers.forEach(clearTimeout);
  timers = [];
  running = false;
}

function typeLoop(i: number) {
  const cmd = COMMANDS[i % COMMANDS.length]!;
  let n = 0;
  typed.value = '';
  const step = () => {
    n += 1;
    typed.value = cmd.slice(0, n);
    if (n < cmd.length) later(step, 38 + Math.random() * 50);
    else
      later(() => {
        history.value = [...history.value, cmd].slice(-3);
        typeLoop(i + 1);
      }, 1300);
  };
  later(step, 300);
}
function snippetLoop() {
  later(() => {
    service.value = (service.value + 1) % SERVICES.length;
    snippetLoop();
  }, 1800);
}
function aiLoop() {
  const full = aiCommand.value;
  aiShown.value = '';
  let n = 0;
  const step = () => {
    n += 2;
    aiShown.value = full.slice(0, n);
    if (n < full.length) later(step, 40);
    else later(aiLoop, 3200);
  };
  later(step, 1400);
}
function start() {
  if (running || prefersReducedMotion()) return;
  running = true;
  typeLoop(1);
  snippetLoop();
  aiLoop();
}

watch(inView, (v) => (v ? start() : stop()));
onMounted(() => {
  // SVG (SMIL) motion ignores CSS, so stop it explicitly for reduced motion.
  if (prefersReducedMotion()) routeSvg.value?.pauseAnimations();
  if (inView.value) start();
});
onBeforeUnmount(stop);

const serviceName = computed(() => SERVICES[service.value]);
</script>

<template>
  <section id="features" class="features">
    <div class="container">
      <div class="head">
        <p class="eyebrow" v-reveal>Everything in one app</p>
        <h2 class="h-section" v-reveal="80">
          Every tool you reach for. <span class="h-soft">None you have to trust.</span>
        </h2>
      </div>

      <div ref="grid" class="bento" :class="{ live: inView }">
        <!-- Protocols -->
        <article v-spotlight v-reveal class="card span-2">
          <div class="visual term">
            <div class="term-bar"><span /><span /><span /><em class="mono">vaultport</em></div>
            <div class="term-body mono">
              <p v-for="(h, i) in history" :key="i + h" class="done">
                <span class="ps">❯</span> {{ h }} <span class="ok">✓ connected</span>
              </p>
              <p><span class="ps">❯</span> {{ typed }}<span class="caret" /></p>
            </div>
          </div>
          <div class="text">
            <h3><SquareTerminal :size="18" /> Every protocol</h3>
            <p>
              SSH, SFTP, Mosh, Telnet, serial consoles and a local shell. Mosh is built in — no
              external binary.
            </p>
          </div>
        </article>

        <!-- Jump hosts -->
        <article v-spotlight v-reveal="80" class="card span-2">
          <div class="visual route">
            <svg ref="routeSvg" viewBox="0 0 420 120" class="route-svg" aria-hidden="true">
              <defs>
                <linearGradient id="fg-route" x1="0" x2="1">
                  <stop offset="0" style="stop-color: rgb(var(--glow-1))" />
                  <stop offset="1" style="stop-color: rgb(var(--glow-2))" />
                </linearGradient>
              </defs>
              <path
                id="fg-path"
                d="M50 60 C 120 10, 160 10, 210 60 S 300 110, 370 60"
                class="route-line"
              />
              <circle r="5" class="packet">
                <animateMotion dur="2.8s" repeatCount="indefinite" rotate="auto">
                  <mpath href="#fg-path" />
                </animateMotion>
              </circle>
              <circle r="5" class="packet b">
                <animateMotion dur="2.8s" begin="1.4s" repeatCount="indefinite">
                  <mpath href="#fg-path" />
                </animateMotion>
              </circle>
            </svg>
            <div class="node n1"><Laptop :size="18" /><span>you</span></div>
            <div class="node n2"><Waypoints :size="18" /><span>bastion</span></div>
            <div class="node n3"><Server :size="18" /><span>db-prod</span></div>
          </div>
          <div class="text">
            <h3><Waypoints :size="18" /> Jump hosts &amp; proxies</h3>
            <p>
              Chain bastions with ProxyJump, go through HTTP CONNECT or SOCKS5, and check the host
              key at every hop.
            </p>
          </div>
        </article>

        <!-- Hardware keys -->
        <article v-spotlight v-reveal class="card">
          <div class="visual key">
            <div class="key-orb">
              <span class="r1 ring" /><span class="r2 ring" /><span class="r3 ring" />
              <Fingerprint :size="30" />
            </div>
            <p class="mono key-hint">Touch your security key…</p>
          </div>
          <div class="text">
            <h3><KeyRound :size="18" /> Hardware-backed keys</h3>
            <p>
              FIDO2 <code>sk-</code> keys, plus non-exportable keys in the Secure Enclave, Windows
              Hello or Android Keystore.
            </p>
          </div>
        </article>

        <!-- Post-quantum -->
        <article v-spotlight v-reveal="80" class="card">
          <div class="visual pq">
            <div class="orbit">
              <span class="atom" /><span class="e e1" /><span class="e e2" /><span class="e e3" />
            </div>
            <p class="mono pq-name">mlkem768x25519-sha256</p>
          </div>
          <div class="text">
            <h3><ShieldCheck :size="18" /> Post-quantum by default</h3>
            <p>
              Hybrid ML-KEM key exchange first; legacy algorithms stay off unless you opt in per
              host.
            </p>
          </div>
        </article>

        <!-- Port forwarding -->
        <article v-spotlight v-reveal="160" class="card span-2">
          <div class="visual fwd mono">
            <div class="fwd-row">
              <b>L</b><span>localhost:8080</span><i class="flow" /><span>web-staging:80</span>
            </div>
            <div class="fwd-row">
              <b>R</b><span>bastion:9000</span><i class="flow rev" /><span>localhost:3000</span>
            </div>
            <div class="fwd-row">
              <b>D</b><span>localhost:1080</span><i class="flow" /><span>SOCKS5 · any</span>
            </div>
          </div>
          <div class="text">
            <h3><ArrowRight :size="18" /> Port forwarding</h3>
            <p>
              Local, remote and dynamic tunnels as saved rules. Start them with a double-click or
              automatically on connect.
            </p>
          </div>
        </article>

        <!-- Snippets -->
        <article v-spotlight v-reveal class="card span-2">
          <div class="visual snippet mono">
            <span class="ps">❯</span> sudo systemctl restart
            <span class="var">
              <Transition name="flip" mode="out-in">
                <span :key="serviceName">{{ serviceName }}</span>
              </Transition>
            </span>
            <div v-pre class="snippet-meta">{{service:nginx}} · run in 4 tabs</div>
          </div>
          <div class="text">
            <h3><History :size="18" /> Snippets &amp; autocomplete</h3>
            <p>
              Reusable commands with <code v-pre>{{ variables }}</code>, run in one tab or many, and
              offline completion that knows the commands, branches and scripts on the machine.
            </p>
          </div>
        </article>

        <!-- Local AI -->
        <article v-spotlight v-reveal="80" class="card span-2">
          <div class="visual ai">
            <p class="ai-prompt"><Sparkles :size="14" /> show the five biggest files in /var/log</p>
            <p class="ai-out mono"><span class="ps">❯</span> {{ aiShown }}<span class="caret" /></p>
            <p class="ai-note mono">inserted, never run · model runs on this device</p>
          </div>
          <div class="text">
            <h3><Cpu :size="18" /> Local AI, off by default</h3>
            <p>
              Describe what you want and get a shell command from a small model running entirely on
              your device.
            </p>
          </div>
        </article>

        <!-- Session logs -->
        <article v-spotlight v-reveal class="card span-2">
          <div class="visual logs">
            <div class="bars">
              <span
                v-for="i in 48"
                :key="i"
                :style="{ '--h': `${18 + ((i * 37) % 70)}%`, '--d': `${i * 40}ms` }"
              />
            </div>
            <span class="playhead" />
            <p class="mono logs-meta">
              <span>▶ replay</span><span>prod-deploy.vplog · sealed</span>
            </p>
          </div>
          <div class="text">
            <h3><History :size="18" /> Encrypted session logs</h3>
            <p>
              Record sessions as sealed, tamper-evident logs and replay them later, keystroke by
              keystroke.
            </p>
          </div>
        </article>

        <!-- Import -->
        <article v-spotlight v-reveal="80" class="card span-2">
          <div class="visual import">
            <div class="chips">
              <span
                v-for="s in [
                  '~/.ssh/config',
                  'known_hosts',
                  'PuTTY',
                  'MobaXterm',
                  'SecureCRT',
                  'CSV',
                  'AWS',
                  'DigitalOcean',
                  'Azure',
                ]"
                :key="s"
                class="mono"
                >{{ s }}</span
              >
            </div>
          </div>
          <div class="text">
            <h3><FileInput :size="18" /> Bring everything with you</h3>
            <p>
              Import your existing setup in one go. Cloud inventories are read with credentials that
              never leave your device.
            </p>
          </div>
        </article>
      </div>

      <a :href="withBase('/guide/')" class="all-link" v-reveal
        >Explore every feature in the docs <ArrowRight :size="16"
      /></a>
    </div>
  </section>
</template>

<style scoped>
.features {
  padding: 120px 0 60px;
}
.head {
  display: grid;
  gap: 18px;
  max-width: 900px;
  margin-bottom: 48px;
}
.bento {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
}
.card {
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border);
  background: linear-gradient(180deg, var(--surface-2), var(--surface) 40%);
  overflow: hidden;
  transition:
    border-color 0.3s,
    transform 0.5s var(--ease-out);
}
.card:hover {
  border-color: var(--border-strong);
}
.span-2 {
  grid-column: span 2;
}
.visual {
  position: relative;
  height: 190px;
  margin: 10px 10px 0;
  border-radius: calc(var(--radius-lg) - 8px);
  background: var(--bg-soft);
  border: 1px solid var(--border);
  overflow: hidden;
}
.text {
  padding: 20px 22px 24px;
}
.text h3 {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 17px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.text h3 svg {
  color: var(--accent);
}
.text p {
  margin-top: 8px;
  font-size: 14.5px;
  line-height: 1.6;
  color: var(--text-2);
}
.text code {
  font-family: var(--font-mono);
  font-size: 0.88em;
  color: var(--accent-strong);
}
.ps {
  color: var(--ok);
}
.caret {
  display: inline-block;
  width: 0.55em;
  height: 1.1em;
  margin-left: 2px;
  vertical-align: -0.2em;
  background: var(--accent);
  animation: blink 1s steps(1) infinite;
}
@keyframes blink {
  50% {
    opacity: 0;
  }
}

/* Terminal */
.term-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.term-bar span {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--surface-3);
}
.term-bar em {
  margin-left: auto;
  font-style: normal;
  font-size: 12px;
  color: var(--text-3);
}
.term-body {
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: 6px;
  height: calc(100% - 36px);
  padding: 14px 16px;
  font-size: 13px;
  background:
    radial-gradient(60% 80% at 10% 0%, rgb(var(--glow-1) / 0.18), transparent 70%),
    radial-gradient(50% 70% at 100% 100%, rgb(var(--glow-2) / 0.12), transparent 70%);
}
.done {
  color: var(--text-3);
  animation: line-in 0.4s var(--ease-out);
}
.ok {
  color: var(--ok);
  font-size: 12px;
}
@keyframes line-in {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
}

/* Route */
.route-svg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
}
.route-line {
  fill: none;
  stroke: url(#fg-route);
  stroke-width: 2;
  stroke-dasharray: 5 7;
  animation: dash 1.2s linear infinite;
}
@keyframes dash {
  to {
    stroke-dashoffset: -24;
  }
}
.packet {
  fill: rgb(var(--glow-1));
  filter: drop-shadow(0 0 6px rgb(var(--glow-1)));
}
.packet.b {
  fill: rgb(var(--glow-2));
}
.node {
  position: absolute;
  top: 50%;
  display: grid;
  justify-items: center;
  gap: 6px;
  transform: translate(-50%, -50%);
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-2);
}
.node svg {
  box-sizing: content-box;
  padding: 10px;
  border-radius: 12px;
  background: var(--bg-elev);
  border: 1px solid var(--border-strong);
  color: var(--text);
}
.n1 {
  left: 11.9%;
}
.n2 {
  left: 50%;
}
.n2 svg {
  color: var(--accent);
  box-shadow: 0 0 20px rgb(var(--glow-1) / 0.4);
}
.n3 {
  left: 88.1%;
}

/* Hardware key */
.key {
  display: grid;
  place-items: center;
  align-content: center;
  gap: 16px;
}
.key-orb {
  position: relative;
  display: grid;
  place-items: center;
  width: 72px;
  height: 72px;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent);
}
.ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 1px solid var(--accent);
  opacity: 0;
}
.live .ring {
  animation: ripple 2.4s var(--ease-out) infinite;
}
.r2 {
  animation-delay: 0.8s !important;
}
.r3 {
  animation-delay: 1.6s !important;
}
@keyframes ripple {
  from {
    opacity: 0.8;
    transform: scale(1);
  }
  to {
    opacity: 0;
    transform: scale(2.2);
  }
}
.key-hint {
  font-size: 12px;
  color: var(--text-3);
}

/* Post-quantum */
.pq {
  display: grid;
  place-items: center;
  align-content: center;
  gap: 18px;
}
.orbit {
  position: relative;
  width: 84px;
  height: 84px;
}
.atom {
  position: absolute;
  inset: 34px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 20px var(--accent);
}
.e {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  animation: orbit 3s linear infinite;
}
.e::after {
  content: '';
  position: absolute;
  top: -3px;
  left: 50%;
  width: 6px;
  height: 6px;
  margin-left: -3px;
  border-radius: 50%;
  background: var(--accent-2);
  box-shadow: 0 0 8px var(--accent-2);
}
.e1 {
  transform: rotateX(70deg);
}
.e2 {
  transform: rotateX(70deg) rotateY(60deg);
  animation-duration: 4s;
}
.e3 {
  transform: rotateX(70deg) rotateY(-60deg);
  animation-duration: 5s;
}
@keyframes orbit {
  to {
    rotate: z 360deg;
  }
}
.pq-name {
  font-size: 12px;
  color: var(--text-2);
}

/* Forwarding */
.fwd {
  display: grid;
  align-content: center;
  gap: 12px;
  padding: 0 18px;
  font-size: 12.5px;
}
.fwd-row {
  display: grid;
  grid-template-columns: 26px auto 1fr auto;
  align-items: center;
  gap: 12px;
  color: var(--text-2);
}
.fwd-row b {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.flow {
  height: 2px;
  border-radius: 2px;
  background: repeating-linear-gradient(90deg, var(--accent) 0 8px, transparent 8px 16px);
  background-size: 32px 2px;
  animation: flow 0.8s linear infinite;
}
.flow.rev {
  background: repeating-linear-gradient(90deg, var(--accent-2) 0 8px, transparent 8px 16px);
  background-size: 32px 2px;
  animation-direction: reverse;
}
@keyframes flow {
  to {
    background-position: 32px 0;
  }
}

/* Snippets */
.snippet {
  display: grid;
  align-content: center;
  justify-content: center;
  padding: 0 20px;
  font-size: clamp(13px, 1.3vw, 16px);
  text-align: center;
}
.var {
  display: inline-block;
  margin-left: 6px;
  padding: 2px 10px;
  border-radius: 8px;
  background: var(--accent-soft);
  color: var(--accent-strong);
  border: 1px solid color-mix(in oklab, var(--accent) 40%, transparent);
}
.snippet-meta {
  margin-top: 14px;
  font-size: 12px;
  color: var(--text-3);
}
.flip-enter-active,
.flip-leave-active {
  display: inline-block;
  transition:
    transform 0.35s var(--ease-out),
    opacity 0.25s;
}
.flip-enter-from {
  transform: translateY(70%) rotateX(-60deg);
  opacity: 0;
}
.flip-leave-to {
  transform: translateY(-70%) rotateX(60deg);
  opacity: 0;
}

/* Local AI */
.ai {
  display: grid;
  align-content: center;
  gap: 12px;
  padding: 0 20px;
}
.ai-prompt {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  width: fit-content;
  padding: 8px 14px;
  border-radius: 999px;
  border: 1px solid rgb(var(--glow-3) / 0.4);
  background: rgb(var(--glow-3) / 0.08);
  font-size: 13px;
  color: var(--text);
}
.ai-prompt svg {
  color: rgb(var(--glow-3));
}
.ai-out {
  font-size: 13.5px;
  min-height: 1.6em;
}
.ai-note {
  font-size: 12px;
  color: var(--text-3);
}

/* Logs */
.logs {
  padding: 22px 18px 0;
}
.bars {
  display: flex;
  align-items: flex-end;
  gap: 3px;
  height: 110px;
}
.bars span {
  flex: 1;
  height: var(--h);
  border-radius: 2px;
  background: linear-gradient(to top, rgb(var(--glow-1) / 0.25), rgb(var(--glow-1) / 0.8));
  transform-origin: bottom;
}
.live .bars span {
  animation: bar 1.6s var(--ease-in-out) infinite alternate;
  animation-delay: var(--d);
}
@keyframes bar {
  to {
    transform: scaleY(0.45);
  }
}
.playhead {
  position: absolute;
  top: 14px;
  bottom: 44px;
  left: 18px;
  width: 2px;
  background: var(--accent-2);
  box-shadow: 0 0 10px var(--accent-2);
}
.live .playhead {
  animation: play 6s linear infinite;
}
@keyframes play {
  to {
    left: calc(100% - 20px);
  }
}
.logs-meta {
  display: flex;
  justify-content: space-between;
  margin-top: 16px;
  font-size: 12px;
  color: var(--text-3);
}

/* Import */
.import {
  display: grid;
  place-items: center;
  padding: 16px;
}
.chips {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 8px;
}
.chips span {
  padding: 6px 12px;
  border-radius: 999px;
  border: 1px solid var(--border);
  background: var(--bg-elev);
  font-size: 12px;
  color: var(--text-2);
  transition:
    transform 0.4s var(--ease-spring),
    color 0.2s,
    border-color 0.2s;
}
.live .chips span {
  animation: bob 3s var(--ease-in-out) infinite;
}
.chips span:nth-child(odd) {
  animation-delay: -1.5s !important;
}
.chips span:hover {
  transform: translateY(-4px) scale(1.06);
  color: var(--text);
  border-color: var(--accent);
}
@keyframes bob {
  50% {
    translate: 0 -4px;
  }
}

.all-link {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-top: 28px;
  font-weight: 550;
  color: var(--accent);
}
.all-link svg {
  transition: transform 0.3s var(--ease-out);
}
.all-link:hover svg {
  transform: translateX(4px);
}

@media (max-width: 1000px) {
  .bento {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 640px) {
  .bento {
    grid-template-columns: minmax(0, 1fr);
  }
  .span-2 {
    grid-column: auto;
  }
}
</style>
