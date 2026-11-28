<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script setup lang="ts">
/**
 * A WebGL aurora: domain-warped noise in the brand colours that drifts slowly and leans toward
 * the pointer. It renders at a fraction of the screen resolution (it is soft anyway), pauses
 * off screen or in a background tab, and draws a single still frame for reduced motion. Without
 * WebGL the CSS gradient underneath stays.
 */
import { useData } from 'vitepress';
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { prefersReducedMotion } from '../../composables/motion';

const canvas = ref<HTMLCanvasElement | null>(null);
const ready = ref(false);
const { isDark } = useData();

const VERT = `attribute vec2 p; void main(){ gl_Position = vec4(p, 0.0, 1.0); }`;
const FRAG = `
precision highp float;
uniform vec2 uRes;
uniform float uTime;
uniform vec2 uMouse;
uniform float uLight;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
float fbm(vec2 p) {
  float v = 0.0, a = 0.5;
  mat2 m = mat2(1.6, 1.2, -1.2, 1.6);
  for (int i = 0; i < 5; i++) { v += a * noise(p); p = m * p; a *= 0.5; }
  return v;
}
void main() {
  vec2 uv = gl_FragCoord.xy / uRes;
  vec2 p = (gl_FragCoord.xy - 0.5 * uRes) / uRes.y;
  float t = uTime * 0.045;
  vec2 m = (uMouse - 0.5) * vec2(uRes.x / uRes.y, 1.0);
  vec2 q = vec2(fbm(p * 1.3 + t), fbm(p * 1.3 - t + 3.1));
  vec2 r = vec2(
    fbm(p * 1.5 + 2.0 * q + vec2(1.7, 9.2) + t * 1.3 + m * 0.35),
    fbm(p * 1.5 + 2.0 * q + vec2(8.3, 2.8) - t)
  );
  float f = fbm(p * 1.1 + 2.3 * r);
  vec3 iris = vec3(0.48, 0.40, 0.96);
  vec3 cyan = vec3(0.33, 0.82, 0.95);
  vec3 rose = vec3(1.0, 0.48, 0.72);
  vec3 col = mix(iris, cyan, clamp(r.x * 1.5 - 0.35, 0.0, 1.0));
  col = mix(col, rose, clamp(q.y * q.y * 1.4 - 0.3, 0.0, 1.0) * 0.55);
  float intensity = smoothstep(0.3, 0.95, f);
  float band = smoothstep(1.15, 0.0, length((p - vec2(0.0, 0.42)) * vec2(0.55, 1.35)));
  float glow = exp(-3.5 * length(p - m));
  float a = intensity * band + glow * 0.22 * intensity;
  vec3 bg = mix(vec3(0.027, 0.024, 0.043), vec3(0.984, 0.98, 1.0), uLight);
  vec3 outc = mix(bg, col, clamp(a, 0.0, 1.0) * mix(0.9, 0.42, uLight));
  outc *= mix(1.0, 0.82, smoothstep(0.35, 1.15, length(uv - 0.5)) * (1.0 - uLight));
  gl_FragColor = vec4(outc, 1.0);
}`;

let gl: WebGLRenderingContext | null = null;
let raf = 0;
let running = false;
let visible = true;
let start = 0;
let uniforms: Record<string, WebGLUniformLocation | null> = {};
const mouse = { x: 0.5, y: 0.65, tx: 0.5, ty: 0.65 };
let observer: IntersectionObserver | null = null;
let resizeObserver: ResizeObserver | null = null;

function compile(type: number, src: string) {
  const s = gl!.createShader(type)!;
  gl!.shaderSource(s, src);
  gl!.compileShader(s);
  return s;
}

function resize() {
  const c = canvas.value;
  if (!c || !gl) return;
  const scale = Math.min(window.devicePixelRatio || 1, 2) * 0.45;
  c.width = Math.max(1, Math.round(c.clientWidth * scale));
  c.height = Math.max(1, Math.round(c.clientHeight * scale));
  gl.viewport(0, 0, c.width, c.height);
}

function draw(now: number) {
  if (!gl || !canvas.value) return;
  mouse.x += (mouse.tx - mouse.x) * 0.04;
  mouse.y += (mouse.ty - mouse.y) * 0.04;
  gl.uniform2f(uniforms.uRes!, canvas.value.width, canvas.value.height);
  gl.uniform1f(uniforms.uTime!, (now - start) / 1000 + 20);
  gl.uniform2f(uniforms.uMouse!, mouse.x, mouse.y);
  gl.uniform1f(uniforms.uLight!, isDark.value ? 0 : 1);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}

function loop(now: number) {
  draw(now);
  raf = running ? requestAnimationFrame(loop) : 0;
}
function play() {
  if (running || prefersReducedMotion() || !visible || document.hidden) return;
  running = true;
  raf = requestAnimationFrame(loop);
}
function pause() {
  running = false;
  cancelAnimationFrame(raf);
}

function onPointer(e: PointerEvent) {
  mouse.tx = e.clientX / window.innerWidth;
  mouse.ty = 1 - e.clientY / window.innerHeight;
}
function onVisibility() {
  if (document.hidden) pause();
  else play();
}

onMounted(() => {
  const c = canvas.value;
  if (!c) return;
  gl = c.getContext('webgl', { antialias: false, alpha: false, powerPreference: 'low-power' });
  if (!gl) return;
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compile(gl.VERTEX_SHADER, VERT));
  gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return;
  gl.useProgram(prog);
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const loc = gl.getAttribLocation(prog, 'p');
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  uniforms = Object.fromEntries(
    ['uRes', 'uTime', 'uMouse', 'uLight'].map((n) => [n, gl!.getUniformLocation(prog, n)]),
  );
  start = performance.now();
  resize();
  draw(start);
  ready.value = true;

  resizeObserver = new ResizeObserver(() => {
    resize();
    if (!running) draw(performance.now());
  });
  resizeObserver.observe(c);
  observer = new IntersectionObserver(([entry]) => {
    visible = Boolean(entry?.isIntersecting);
    if (visible) play();
    else pause();
  });
  observer.observe(c);
  window.addEventListener('pointermove', onPointer, { passive: true });
  document.addEventListener('visibilitychange', onVisibility);
  play();
});

watch(isDark, () => {
  if (!running) requestAnimationFrame((t) => draw(t));
});

onBeforeUnmount(() => {
  pause();
  observer?.disconnect();
  resizeObserver?.disconnect();
  window.removeEventListener('pointermove', onPointer);
  document.removeEventListener('visibilitychange', onVisibility);
  gl?.getExtension('WEBGL_lose_context')?.loseContext();
});
</script>

<template>
  <div class="backdrop" aria-hidden="true">
    <canvas ref="canvas" :class="{ ready }" />
  </div>
</template>

<style scoped>
.backdrop {
  position: absolute;
  inset: 0;
  overflow: hidden;
  background:
    radial-gradient(50% 45% at 30% 22%, rgb(var(--glow-1) / 0.4), transparent 70%),
    radial-gradient(40% 40% at 72% 30%, rgb(var(--glow-2) / 0.28), transparent 70%), var(--bg);
  mask-image: linear-gradient(to bottom, #000 55%, transparent);
  -webkit-mask-image: linear-gradient(to bottom, #000 55%, transparent);
}
canvas {
  width: 100%;
  height: 100%;
  opacity: 0;
  transition: opacity 1.6s var(--ease-out);
}
canvas.ready {
  opacity: 1;
}
</style>
