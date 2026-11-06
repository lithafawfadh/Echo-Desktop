<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../stores/playerState.svelte';
  import { createLyricsClock } from '../utils/lyricsClock.svelte';
  import { LyricSpring } from '../utils/springPhysics';

  const clock = createLyricsClock();
  const spring = new LyricSpring();

  let lyricsContainerRef: HTMLDivElement;
  let rafHandle: number | null = null;
  let lastFrameTime = performance.now();

  // Mouse tilt for 3D vinyl cover
  let tiltX = $state(0);
  let tiltY = $state(0);

  // Active lyrics source: either from player or fallback structured demo
  let activeLyrics = $derived(
    player.currentLyrics?.synced && player.currentLyrics.synced.length > 0
      ? player.currentLyrics.synced
      : [
          { timestamp_ms: 0, text: 'Echo Desktop — Lossless Audio Pipeline' },
          { timestamp_ms: 4000, text: 'Caught in the echo of the night' },
          { timestamp_ms: 8500, text: 'Sub-atomic waves in crystal glass' },
          { timestamp_ms: 13000, text: 'We trace the sound until it lasts' },
          { timestamp_ms: 18000, text: 'Nothing remains but the bassline groove' },
          { timestamp_ms: 24000, text: 'Stream resolved at ninety-six kilohertz' },
          { timestamp_ms: 30000, text: 'Zero distortion in the matrix flow' },
        ]
  );

  let activeIndex = $derived.by(() => {
    const time = clock.currentMs;
    const lines = activeLyrics;
    let idx = -1;
    for (let i = 0; i < lines.length; i++) {
      if (time >= lines[i].timestamp_ms) {
        idx = i;
      } else {
        break;
      }
    }
    return idx;
  });

  // Keep lyrics clock synchronized whenever player state updates (seeks, play, pause)
  $effect(() => {
    clock.updateAnchor(player.positionMs, player.isPlaying);
  });

  function getWordProgress(lineIdx: number, wordIdx: number, totalWords: number): number {
    if (lineIdx < activeIndex) return 100;
    if (lineIdx > activeIndex) return 0;

    const line = activeLyrics[lineIdx];
    const nextLine = activeLyrics[lineIdx + 1];
    const lineDur = nextLine ? Math.max(1200, nextLine.timestamp_ms - line.timestamp_ms) : 4000;
    const wordDur = lineDur / Math.max(1, totalWords);
    const wordStart = line.timestamp_ms + wordIdx * wordDur;
    const wordEnd = wordStart + wordDur;
    const cur = clock.currentMs;

    if (cur <= wordStart) return 0;
    if (cur >= wordEnd) return 100;
    return Math.min(100, Math.max(0, ((cur - wordStart) / wordDur) * 100));
  }

  async function handleLineClick(timestamp_ms: number) {
    player.positionMs = timestamp_ms;
    clock.updateAnchor(timestamp_ms, player.isPlaying);
    try {
      await invoke('seek_to', { positionMs: timestamp_ms });
    } catch (e) {
      console.warn('Seek IPC:', e);
    }
  }

  function handleMouseMove(e: MouseEvent) {
    const { clientX, clientY } = e;
    const { innerWidth, innerHeight } = window;
    tiltX = -((clientY / innerHeight) - 0.5) * 16;
    tiltY = ((clientX / innerWidth) - 0.5) * 16;
  }

  function handleMouseLeave() {
    tiltX = 0;
    tiltY = 0;
  }

  function physicsLoop(now: number) {
    const dt = (now - lastFrameTime) / 1000;
    lastFrameTime = now;

    // Calculate target scroll offset based on active line
    if (lyricsContainerRef) {
      const lineElements = lyricsContainerRef.querySelectorAll('.lyric-line');
      if (lineElements.length > 0 && activeIndex >= 0 && activeIndex < lineElements.length) {
        const activeEl = lineElements[activeIndex] as HTMLElement;
        const containerH = lyricsContainerRef.clientHeight;
        const elTop = activeEl.offsetTop;
        const elH = activeEl.clientHeight;
        spring.target = -(elTop - containerH / 2 + elH / 2);
      }
    }

    spring.step(dt);
    if (lyricsContainerRef) {
      lyricsContainerRef.style.transform = `translate3d(0, ${spring.x}px, 0)`;
    }

    rafHandle = requestAnimationFrame(physicsLoop);
  }

  onMount(() => {
    clock.updateAnchor(player.positionMs, player.isPlaying);
    clock.start();
    lastFrameTime = performance.now();
    rafHandle = requestAnimationFrame(physicsLoop);
  });

  onDestroy(() => {
    clock.stop();
    if (rafHandle !== null) cancelAnimationFrame(rafHandle);
  });
</script>

<div
  class="echo-fullscreen-stage"
  onmousemove={handleMouseMove}
  onmouseleave={handleMouseLeave}
  role="dialog"
  tabindex="-1"
  aria-modal="true"
  aria-label="Full-Screen Immersive Lyrics"
>
  <button
    class="close-btn"
    onclick={() => (player.isFullScreenLyrics = false)}
    aria-label="Close Fullscreen View"
  >
    ✕ CLOSE
  </button>

  <div class="stage-split">
    <!-- Left: 3D Artwork Viewport -->
    <div class="artwork-column">
      <div
        class="vinyl-sleeve"
        style="transform: perspective(900px) rotateX({tiltX}deg) rotateY({tiltY}deg)"
      >
        {#if player.currentTrack?.thumbnail_url}
          <img src={player.currentTrack.thumbnail_url} alt="Cover artwork" class="big-cover" />
        {:else}
          <div class="big-cover-placeholder">
            <span class="cover-symbol">♪</span>
          </div>
        {/if}
        <div class="sleeve-specular-sheen" aria-hidden="true"></div>
      </div>

      <div class="stage-meta">
        <h1 class="track-title">{player.currentTrack?.title ?? 'Echo Music'}</h1>
        <h2 class="artist-name">{player.currentTrack?.artist ?? 'Lossless Audio'}</h2>

        {#if player.currentTrack && player.fidelityBadge}
          <div
            class="hi-res-banner"
            style="--badge-color: {player.fidelityBadge.color}; --badge-bg: {player.fidelityBadge.bg}; --badge-border: {player.fidelityBadge.border}; --badge-glow: {player.fidelityBadge.glow};"
          >
            <span class="matrix-tag">
              <span class="banner-dot" class:buffering={player.isBuffering}></span>
              {player.fidelityBadge.bannerTitle}
            </span>
            <span class="matrix-rate">
              {#if player.currentTrack.bit_depth}
                {((player.currentTrack.sample_rate_hz || 44100) / 1000).toFixed(1)} kHz / {player.currentTrack.bit_depth}-BIT • {(player.currentTrack.codec || 'FLAC').toUpperCase()}
              {:else if player.currentTrack.is_lossless}
                {((player.currentTrack.sample_rate_hz || 44100) / 1000).toFixed(1)} kHz / 16-BIT • {(player.currentTrack.codec || 'FLAC').toUpperCase()}
              {:else}
                {player.currentTrack.bitrate_kbps || 160} KBPS • {(player.currentTrack.codec || 'OPUS').toUpperCase()}
              {/if}
            </span>
            <span class="fidelity-pill">
              {player.fidelityBadge.label}
            </span>
          </div>
        {/if}
      </div>
    </div>

    <!-- Right: Fluid Word & Line Karaoke View with RK4 Spring Physics -->
    <div class="lyrics-viewport">
      <div bind:this={lyricsContainerRef} class="lyrics-motion-wrapper">
        {#each activeLyrics as line, index}
          {@const words = line.text.split(' ')}
          <div
            class="lyric-line"
            class:active={index === activeIndex}
            class:past={index < activeIndex}
            class:future={index > activeIndex}
            onclick={() => handleLineClick(line.timestamp_ms)}
            role="button"
            tabindex="0"
            onkeydown={(e) => e.key === 'Enter' && handleLineClick(line.timestamp_ms)}
            title="Click to seek to this line"
          >
            {#each words as word, wIdx}
              {@const progress = getWordProgress(index, wIdx, words.length)}
              <span
                class="echo-syllable-word"
                class:active={index === activeIndex && progress > 0 && progress < 100}
                style="--word-progress: {progress.toFixed(1)}%;"
              >{word}&nbsp;</span>
            {/each}
          </div>
        {/each}
      </div>
    </div>
  </div>
</div>

<style>
  .echo-fullscreen-stage {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(4, 4, 6, 0.88);
    backdrop-filter: blur(48px) saturate(200%);
    -webkit-backdrop-filter: blur(48px) saturate(200%);
    display: flex;
    flex-direction: column;
    padding: var(--echo-spacing-xl);
    animation: fadeIn 0.3s var(--echo-spring-smooth);
    user-select: none;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: scale(0.98); }
    to { opacity: 1; transform: scale(1); }
  }

  .close-btn {
    align-self: flex-end;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--echo-radius-full);
    color: #fff;
    padding: 8px 18px;
    font-family: "NDot57", monospace;
    font-size: 11px;
    letter-spacing: 0.1em;
    cursor: pointer;
    transition: all 0.2s;
  }

  .close-btn:hover {
    background: var(--echo-color-accent);
    border-color: var(--echo-color-accent);
    transform: scale(1.05);
  }

  .stage-split {
    display: grid;
    grid-template-columns: 460px 1fr;
    gap: 72px;
    flex: 1;
    align-items: center;
    max-width: 1380px;
    margin: 0 auto;
    width: 100%;
    overflow: hidden;
  }

  .artwork-column {
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .vinyl-sleeve {
    position: relative;
    width: 420px;
    height: 420px;
    border-radius: 28px;
    overflow: hidden;
    box-shadow: 0 32px 70px -16px rgba(0, 0, 0, 0.85),
                0 0 0 1px rgba(255, 255, 255, 0.14);
    transition: transform 0.1s ease-out;
    transform-style: preserve-3d;
  }

  .sleeve-specular-sheen {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      135deg,
      rgba(255, 255, 255, 0.2) 0%,
      transparent 50%,
      rgba(0, 0, 0, 0.3) 100%
    );
    pointer-events: none;
  }

  .big-cover {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .big-cover-placeholder {
    width: 100%;
    height: 100%;
    background: #0d0f14;
    display: grid;
    place-items: center;
  }

  .cover-symbol {
    font-size: 72px;
    color: var(--echo-text-muted);
  }

  .stage-meta {
    display: flex;
    flex-direction: column;
  }

  .track-title {
    font-size: 32px;
    font-weight: 700;
    margin: 0;
    color: #fff;
    letter-spacing: -0.025em;
  }

  .artist-name {
    font-size: 18px;
    color: var(--echo-text-secondary);
    margin: 6px 0 0 0;
    font-weight: 500;
  }

  .hi-res-banner {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
    padding: 5px 12px;
    background: rgba(0, 0, 0, 0.45);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--echo-radius-md);
    width: fit-content;
    backdrop-filter: blur(16px);
    transition: all 0.3s ease;
  }

  .matrix-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: "NDot57", monospace;
    font-size: 9px;
    color: var(--badge-color);
    background: var(--badge-bg);
    border: 1px solid var(--badge-border);
    padding: 3px 8px;
    border-radius: 4px;
    letter-spacing: 0.1em;
    font-weight: 700;
  }

  .banner-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--badge-color);
    box-shadow: 0 0 6px var(--badge-glow);
  }

  .banner-dot.buffering {
    background: var(--echo-status-buffering);
    box-shadow: 0 0 7px var(--echo-status-buffering);
    animation: banner-pulse 1s infinite alternate;
  }

  @keyframes banner-pulse {
    from { opacity: 0.35; transform: scale(0.9); }
    to { opacity: 1; transform: scale(1.15); }
  }

  .matrix-rate {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-primary);
    letter-spacing: 0.06em;
  }

  .fidelity-pill {
    font-family: "NDot57", monospace;
    font-size: 8.5px;
    letter-spacing: 0.08em;
    color: var(--badge-color);
    border: 1px solid var(--badge-border);
    background: var(--badge-bg);
    padding: 2px 7px;
    border-radius: var(--echo-radius-full);
    font-weight: 700;
  }

  .lyrics-viewport {
    height: 64vh;
    overflow: hidden;
    position: relative;
    mask-image: linear-gradient(180deg, transparent 0%, #000 20%, #000 80%, transparent 100%);
    -webkit-mask-image: linear-gradient(180deg, transparent 0%, #000 20%, #000 80%, transparent 100%);
  }

  .lyrics-motion-wrapper {
    will-change: transform;
    display: flex;
    flex-direction: column;
    padding: 30vh 0;
  }

  .lyric-line {
    font-size: 34px;
    font-weight: 700;
    line-height: 1.45;
    transition: transform 0.3s ease, filter 0.3s ease;
    margin-bottom: 28px;
    cursor: pointer;
    border-radius: 8px;
    padding: 4px 8px;
  }

  .lyric-line:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .lyric-line.active {
    transform: scale(1.05) translateX(16px);
  }

  .lyric-line.past {
    opacity: 0.65;
  }

  .lyric-line.future {
    opacity: 0.35;
  }
</style>
