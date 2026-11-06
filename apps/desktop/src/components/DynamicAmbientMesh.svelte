<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  interface Props {
    colors?: [string, string, string, string];
  }

  let { colors = ['#201226', '#0e2238', '#060608', '#401117'] }: Props = $props();

  let canvasRef: HTMLCanvasElement;
  let animId: number | null = null;

  // Interpolated RGB state vectors
  let currentRGB = [
    [32, 18, 38],
    [14, 34, 56],
    [6, 6, 8],
    [64, 17, 23],
  ];
  let targetRGB = $state(currentRGB);

  // Time evolution for organic wave movement
  let t = 0;

  function hexToRgb(hex: string): [number, number, number] {
    const bigint = parseInt(hex.replace('#', ''), 16);
    return [(bigint >> 16) & 255, (bigint >> 8) & 255, bigint & 255];
  }

  $effect(() => {
    if (colors && colors.length >= 4) {
      targetRGB = colors.slice(0, 4).map(hexToRgb);
    }
  });

  function render() {
    if (document.hidden) {
      animId = null;
      return;
    }

    if (!canvasRef) return;
    const ctx = canvasRef.getContext('2d', { alpha: false, desynchronized: true });
    if (!ctx) return;

    // Linear vector interpolation (lerp) for smooth color crossfades
    const lerpSpeed = 0.035;
    for (let i = 0; i < 4; i++) {
      for (let j = 0; j < 3; j++) {
        currentRGB[i][j] += (targetRGB[i][j] - currentRGB[i][j]) * lerpSpeed;
      }
    }

    t += 0.008;
    const w = canvasRef.width;
    const h = canvasRef.height;

    // Draw Dark Base
    ctx.fillStyle = `rgb(${currentRGB[2][0] | 0}, ${currentRGB[2][1] | 0}, ${currentRGB[2][2] | 0})`;
    ctx.fillRect(0, 0, w, h);

    // Orbiting Radial Gradient Blob 1
    const x1 = w * 0.35 + Math.sin(t) * (w * 0.2);
    const y1 = h * 0.3 + Math.cos(t * 0.8) * (h * 0.15);
    const grad1 = ctx.createRadialGradient(x1, y1, 0, x1, y1, w * 0.7);
    grad1.addColorStop(0, `rgba(${currentRGB[0][0] | 0}, ${currentRGB[0][1] | 0}, ${currentRGB[0][2] | 0}, 0.85)`);
    grad1.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = grad1;
    ctx.fillRect(0, 0, w, h);

    // Orbiting Radial Gradient Blob 2
    const x2 = w * 0.75 + Math.cos(t * 0.7) * (w * 0.18);
    const y2 = h * 0.65 + Math.sin(t * 0.9) * (h * 0.2);
    const grad2 = ctx.createRadialGradient(x2, y2, 0, x2, y2, w * 0.65);
    grad2.addColorStop(0, `rgba(${currentRGB[1][0] | 0}, ${currentRGB[1][1] | 0}, ${currentRGB[1][2] | 0}, 0.75)`);
    grad2.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = grad2;
    ctx.fillRect(0, 0, w, h);

    // High Energy Accent Blob 3
    const x3 = w * 0.5 + Math.sin(t * 1.2) * (w * 0.25);
    const y3 = h * 0.85 + Math.cos(t * 1.1) * (h * 0.12);
    const grad3 = ctx.createRadialGradient(x3, y3, 0, x3, y3, w * 0.5);
    grad3.addColorStop(0, `rgba(${currentRGB[3][0] | 0}, ${currentRGB[3][1] | 0}, ${currentRGB[3][2] | 0}, 0.6)`);
    grad3.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = grad3;
    ctx.fillRect(0, 0, w, h);

    animId = requestAnimationFrame(render);
  }

  function handleVisibilityChange() {
    if (document.hidden) {
      if (animId !== null) {
        cancelAnimationFrame(animId);
        animId = null;
      }
    } else {
      if (animId === null && canvasRef) {
        animId = requestAnimationFrame(render);
      }
    }
  }

  onMount(() => {
    if (canvasRef) {
      canvasRef.width = 160;
      canvasRef.height = 90;
      animId = requestAnimationFrame(render);
    }
    document.addEventListener('visibilitychange', handleVisibilityChange);
  });

  onDestroy(() => {
    if (animId !== null) cancelAnimationFrame(animId);
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  });
</script>

<canvas
  bind:this={canvasRef}
  class="ambient-mesh-canvas"
  aria-hidden="true"
></canvas>

<style>
  .ambient-mesh-canvas {
    position: fixed;
    inset: -5%;
    width: 110vw;
    height: 110vh;
    z-index: 0;
    pointer-events: none;
    transform: translate3d(0, 0, 0);
    filter: blur(64px) saturate(180%);
    opacity: 0.75;
    contain: strict;
  }
</style>
