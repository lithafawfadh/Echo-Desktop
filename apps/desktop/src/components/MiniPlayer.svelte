<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../stores/playerState.svelte';

  let isHoveringScrubber = $state(false);
  let hoverFraction = $state(0);

  let hoverTimeDisplay = $derived(
    player.formatTime(hoverFraction * player.durationMs)
  );

  async function togglePlay() {
    if (!player.currentTrack) return;
    try {
      const isNowPlaying = await invoke<boolean>('toggle_playback');
      player.isPlaying = isNowPlaying;
    } catch (err) {
      console.warn('Playback toggle IPC:', err);
      player.isPlaying = !player.isPlaying;
    }
  }

  async function handleSeek(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const targetMs = Math.round(ratio * player.durationMs);
    player.positionMs = targetMs;
    try {
      await invoke('seek_to', { positionMs: targetMs });
    } catch (err) {
      console.warn('Seek IPC:', err);
    }
  }

  function handleScrubberMouseMove(e: MouseEvent) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hoverFraction = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  }

  async function handlePrev() {
    try {
      await invoke('previous_track');
    } catch (err) {
      console.warn('Prev IPC:', err);
    }
  }

  async function handleNext() {
    const next = player.playNext();
    if (next) {
      try {
        await invoke('play_stream', { track: next });
      } catch (err) {
        console.warn('Next IPC:', err);
      }
    } else {
      try {
        await invoke('next_track');
      } catch (err) {
        console.warn('Next IPC:', err);
      }
    }
  }
</script>

<div class="echo-miniplayer-dock">
  <div class="echo-glass-card miniplayer-card">
    <!-- Top-attached sub-pixel progress bar -->
    <div
      class="scrubber-bar"
      onclick={handleSeek}
      onkeydown={(e) => {
        if (e.key === 'ArrowRight') {
          player.positionMs = Math.min(player.durationMs, player.positionMs + 5000);
          invoke('seek_to', { positionMs: player.positionMs }).catch(() => {});
        } else if (e.key === 'ArrowLeft') {
          player.positionMs = Math.max(0, player.positionMs - 5000);
          invoke('seek_to', { positionMs: player.positionMs }).catch(() => {});
        }
      }}
      onmouseenter={() => (isHoveringScrubber = true)}
      onmouseleave={() => (isHoveringScrubber = false)}
      onmousemove={handleScrubberMouseMove}
      role="slider"
      tabindex="0"
      aria-valuenow={player.progressFraction}
      aria-label="Track Progress Scrubber"
    >
      <div
        class="scrubber-fill"
        style="transform: scaleX({player.progressFraction})"
      ></div>
      {#if isHoveringScrubber}
        <div
          class="scrubber-hover-preview"
          style="left: {hoverFraction * 100}%"
        >
          {hoverTimeDisplay}
        </div>
      {/if}
    </div>

    <div class="miniplayer-content">
      <!-- Track Details -->
      <div class="track-identity">
        <div class="thumbnail-squircle">
          {#if player.currentTrack?.thumbnail_url}
            <img src={player.currentTrack.thumbnail_url} alt="Track artwork" loading="lazy" />
          {:else}
            <div class="thumb-placeholder">♪</div>
          {/if}
        </div>
        <div class="meta-stack">
          <div class="title-marquee echo-marquee-container">
            <span class="title-text echo-marquee-content">{player.currentTrack?.title ?? 'Echo Music'}</span>
          </div>
          <span class="artist-text">{player.currentTrack?.artist ?? 'Select a track to stream'}</span>
        </div>
      </div>

      <!-- Center Controls -->
      <div class="playback-controls">
        <button class="ctrl-btn" onclick={handlePrev} aria-label="Previous Track">⏮</button>
        <button
          class="ctrl-btn play-circle"
          class:playing={player.isPlaying}
          onclick={togglePlay}
          aria-label={player.isPlaying ? 'Pause' : 'Play'}
        >
          {player.isPlaying ? '❚❚' : '▶'}
        </button>
        <button class="ctrl-btn" onclick={handleNext} aria-label="Next Track">⏭</button>
      </div>

      <!-- Right Meta & Auxiliary Controls -->
      <div class="aux-cluster">
        <span class="time-readout">
          {player.formattedPosition} <span class="dim">/ {player.formattedDuration}</span>
        </span>
        <button
          class="icon-btn"
          class:active={player.isFullScreenLyrics}
          onclick={() => (player.isFullScreenLyrics = !player.isFullScreenLyrics)}
          title="Word-by-Word Synchronized Lyrics"
          aria-label="Toggle Fullscreen Lyrics"
        >
          ❝
        </button>
        <button
          class="icon-btn"
          class:active={player.isQueueOpen}
          onclick={() => (player.isQueueOpen = !player.isQueueOpen)}
          title="Playback Queue"
          aria-label="Toggle Queue Drawer"
        >
          ≣
        </button>
      </div>
    </div>
  </div>
</div>

<style>
  .echo-miniplayer-dock {
    position: fixed;
    bottom: 16px;
    left: 256px;
    right: 24px;
    height: 72px;
    z-index: 40;
    pointer-events: none;
  }

  .miniplayer-card {
    pointer-events: auto;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    background: rgba(12, 13, 17, 0.78);
    backdrop-filter: var(--echo-blur-glass);
    box-shadow: var(--echo-shadow-dock);
  }

  .scrubber-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    cursor: pointer;
    transition: height 0.15s ease;
  }

  .scrubber-bar:hover {
    height: 6px;
  }

  .scrubber-fill {
    width: 100%;
    height: 100%;
    background: var(--echo-color-accent);
    transform-origin: left center;
    will-change: transform;
    transition: transform 0.05s linear;
  }

  .scrubber-hover-preview {
    position: absolute;
    top: -24px;
    transform: translateX(-50%);
    background: rgba(0, 0, 0, 0.85);
    border: 1px solid rgba(255, 255, 255, 0.15);
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-family: "NDot57", monospace;
    color: #fff;
    pointer-events: none;
  }

  .miniplayer-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--echo-spacing-lg);
  }

  .track-identity {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 280px;
  }

  .thumbnail-squircle {
    width: 44px;
    height: 44px;
    border-radius: var(--echo-radius-sm);
    overflow: hidden;
    background: #000;
    flex-shrink: 0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.5);
  }

  .thumbnail-squircle img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--echo-text-muted);
    font-size: 16px;
  }

  .meta-stack {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .title-marquee {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .title-text {
    font-size: 13px;
    font-weight: 600;
    color: #fff;
    letter-spacing: -0.01em;
  }

  .artist-text {
    font-size: 11px;
    color: var(--echo-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .playback-controls {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .ctrl-btn {
    background: transparent;
    border: none;
    color: #fff;
    cursor: pointer;
    font-size: 14px;
    padding: 6px;
    border-radius: 50%;
    transition: transform 0.15s, opacity 0.15s;
  }

  .ctrl-btn:hover {
    transform: scale(1.1);
  }

  .play-circle {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: #ffffff;
    color: #000000;
    display: grid;
    place-items: center;
    font-size: 13px;
    box-shadow: 0 4px 14px rgba(255, 255, 255, 0.2);
  }

  .play-circle:hover {
    background: #f2f3f5;
    transform: scale(1.06);
  }

  .aux-cluster {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .time-readout {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: #fff;
    letter-spacing: 0.04em;
  }

  .dim {
    color: var(--echo-text-muted);
  }

  .icon-btn {
    background: transparent;
    border: none;
    color: var(--echo-text-secondary);
    cursor: pointer;
    font-size: 16px;
    padding: 6px;
    border-radius: 6px;
    transition: all 0.2s;
  }

  .icon-btn:hover {
    color: #fff;
    background: var(--echo-glass-subtle);
  }

  .icon-btn.active {
    color: var(--echo-color-accent);
    background: var(--echo-glass-medium);
  }
</style>
