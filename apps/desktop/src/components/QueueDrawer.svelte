<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../stores/playerState.svelte';
  import type { TrackMeta } from '../types';

  async function playQueueItem(track: TrackMeta, index: number) {
    player.setTrack(track);
    player.queue = player.queue.filter((_, idx) => idx !== index);
    player.isPlaying = true;
    try {
      await invoke('play_stream', { track });
    } catch (err) {
      console.warn('Play queue item IPC:', err);
    }
  }
</script>

<aside class="echo-queue-drawer" class:open={player.isQueueOpen} aria-label="Playback Queue">
  <div class="drawer-header">
    <div class="header-title">
      <span class="matrix-text">PLAYBACK QUEUE</span>
      <span class="queue-count">{player.queue.length} TRACKS</span>
    </div>
    <div class="flex items-center gap-2">
      {#if player.queue.length > 0}
        <button class="text-btn" onclick={() => player.clearQueue()}>CLEAR</button>
      {/if}
      <button class="clear-btn" onclick={() => (player.isQueueOpen = false)} aria-label="Close Queue">✕</button>
    </div>
  </div>

  <div class="queue-list">
    {#if player.queue.length === 0}
      <div class="empty-queue">
        <span class="empty-icon">≣</span>
        <span class="empty-text">Queue is empty</span>
        <span class="empty-sub">Search or explore tracks to add them</span>
      </div>
    {:else}
      {#each player.queue as track, index}
        <div
          class="queue-item"
          onclick={() => playQueueItem(track, index)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && playQueueItem(track, index)}
        >
          <span class="index-num">{(index + 1).toString().padStart(2, '0')}</span>
          <div class="queue-meta">
            <span class="q-title">{track.title}</span>
            <span class="q-artist">{track.artist}</span>
          </div>
          <span class="q-dur">{player.formatTime(track.duration_seconds * 1000)}</span>
        </div>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .echo-queue-drawer {
    position: fixed;
    top: 48px;
    right: 0;
    bottom: 96px;
    width: 360px;
    background: rgba(12, 13, 17, 0.85);
    backdrop-filter: var(--echo-blur-glass);
    border-left: var(--echo-border-rim);
    transform: translateX(100%);
    transition: transform 0.35s var(--echo-spring-quick);
    z-index: 35;
    display: flex;
    flex-direction: column;
    box-shadow: -16px 0 36px rgba(0, 0, 0, 0.5);
  }

  .echo-queue-drawer.open {
    transform: translateX(0%);
  }

  .drawer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--echo-spacing-md);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .matrix-text {
    font-family: "NDot57", monospace;
    font-size: 13px;
    letter-spacing: 0.08em;
  }

  .queue-count {
    font-family: "NDot57", monospace;
    font-size: 10px;
    color: var(--echo-color-accent);
    margin-left: 8px;
  }

  .text-btn {
    background: transparent;
    border: none;
    color: var(--echo-text-muted);
    font-family: "NDot57", monospace;
    font-size: 10px;
    letter-spacing: 0.06em;
    cursor: pointer;
    padding: 4px 8px;
  }

  .text-btn:hover {
    color: var(--echo-color-accent);
  }

  .clear-btn {
    background: transparent;
    border: none;
    color: var(--echo-text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 4px 8px;
  }

  .clear-btn:hover {
    color: #fff;
  }

  .queue-list {
    flex: 1;
    overflow-y: auto;
    padding: var(--echo-spacing-sm);
  }

  .empty-queue {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--echo-text-muted);
    gap: 8px;
  }

  .empty-icon {
    font-size: 32px;
  }

  .empty-text {
    font-size: 13px;
    font-weight: 500;
  }

  .empty-sub {
    font-size: 11px;
    color: var(--echo-text-disabled);
  }

  .queue-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--echo-radius-sm);
    transition: background 0.15s;
    cursor: pointer;
  }

  .queue-item:hover {
    background: var(--echo-glass-subtle);
  }

  .index-num {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-muted);
    width: 22px;
  }

  .queue-meta {
    flex: 1;
    overflow: hidden;
  }

  .q-title {
    display: block;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: #fff;
  }

  .q-artist {
    display: block;
    font-size: 10px;
    color: var(--echo-text-secondary);
  }

  .q-dur {
    font-family: "NDot57", monospace;
    font-size: 10px;
    color: var(--echo-text-muted);
  }
</style>
