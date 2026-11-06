<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { TrackMeta } from '../types';
  import { player } from '../stores/playerState.svelte';

  interface Props {
    items: TrackMeta[];
  }

  let { items = [] }: Props = $props();

  async function handlePlay(track: TrackMeta) {
    player.setTrack(track);
    player.isPlaying = true;
    try {
      await invoke('play_stream', { track });
    } catch (e) {
      console.warn('VirtualTrackList play_stream IPC:', e);
    }
  }

  let containerRef: HTMLDivElement;
  let scrollTop = $state(0);
  let containerHeight = $state(600);

  const ROW_HEIGHT = 52;
  const OVERSCAN = 3;

  let totalHeight = $derived(items.length * ROW_HEIGHT);

  let startIndex = $derived(
    Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
  );

  let endIndex = $derived(
    Math.min(items.length, Math.ceil((scrollTop + containerHeight) / ROW_HEIGHT) + OVERSCAN)
  );

  let visibleSlice = $derived(
    items.slice(startIndex, endIndex).map((item, idx) => ({
      ...item,
      top: (startIndex + idx) * ROW_HEIGHT,
      originalIndex: startIndex + idx,
    }))
  );

  function handleScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLElement).scrollTop;
  }
</script>

<div
  bind:this={containerRef}
  class="virtual-scroll-viewport"
  onscroll={handleScroll}
>
  <div class="virtual-phantom-height" style="height: {totalHeight}px;">
    {#each visibleSlice as row (row.id)}
      <div
        class="virtual-track-row"
        style="transform: translate3d(0, {row.top}px, 0);"
        onclick={() => handlePlay(row)}
        role="button"
        tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && handlePlay(row)}
      >
        <span class="row-num">{(row.originalIndex + 1).toString().padStart(2, '0')}</span>
        <div class="row-identity">
          <span class="row-title">{row.title}</span>
          <span class="row-artist">{row.artist}</span>
        </div>
        <span class="row-badge">{row.codec.toUpperCase()}</span>
        <span class="row-dur">{player.formatTime(row.duration_seconds * 1000)}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .virtual-scroll-viewport {
    height: 100%;
    overflow-y: auto;
    position: relative;
    contain: strict;
  }

  .virtual-phantom-height {
    position: relative;
    width: 100%;
  }

  .virtual-track-row {
    position: absolute;
    left: 0;
    right: 0;
    height: 52px;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 16px;
    box-sizing: border-box;
    will-change: transform;
    border-radius: var(--echo-radius-md);
    cursor: pointer;
    transition: background 0.1s;
  }

  .virtual-track-row:hover {
    background: var(--echo-glass-subtle);
  }

  .row-num {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-muted);
    width: 24px;
  }

  .row-identity {
    flex: 1;
    overflow: hidden;
  }

  .row-title {
    display: block;
    font-size: 13px;
    font-weight: 600;
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-artist {
    display: block;
    font-size: 11px;
    color: var(--echo-text-secondary);
  }

  .row-badge {
    font-family: "NDot57", monospace;
    font-size: 9px;
    color: var(--echo-lossless-gold);
    background: rgba(255, 255, 255, 0.05);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .row-dur {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-muted);
  }
</style>
