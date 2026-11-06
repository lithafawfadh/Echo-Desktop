<script lang="ts">
  import { player } from '../stores/playerState.svelte';
  import { playTrackWithFallback } from '../lib/playback';
  import type { TrackMeta } from '../types';

  async function handlePlay(track: TrackMeta) {
    player.setTrack(track);
    player.isPlaying = true;
    try {
      await playTrackWithFallback(track);
    } catch (err) {
      console.warn('Play stream error:', err);
    }
  }

  function handleAddToQueue(e: MouseEvent, track: TrackMeta) {
    e.stopPropagation();
    player.addToQueue(track);
  }
</script>

<div class="echo-search-results">
  <header class="results-header">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="title-text">EXPLORE CATALOG</h2>
        <span class="subtitle-text">
          {player.searchResults.length} RESULTS FOR "{player.searchQuery.toUpperCase()}"
        </span>
      </div>
      <button class="back-link" onclick={() => (player.activeTab = 'flow')}>
        ← RETURN TO FLOW
      </button>
    </div>
  </header>

  {#if player.searchResults.length === 0 && !player.isSearching}
    <div class="empty-state">
      <span class="empty-glyph">⚲</span>
      <h3 class="empty-title">NO AUDIO STREAMS FOUND</h3>
      <p class="empty-sub">Try searching for an artist, track title, or album name</p>
    </div>
  {:else}
    <div class="results-list" role="list">
      {#each player.searchResults as track, index}
        <button
          type="button"
          class="track-row text-left w-full"
          onclick={() => handlePlay(track)}
        >
          <span class="row-index">{(index + 1).toString().padStart(2, '0')}</span>

          <div class="row-thumb">
            {#if track.thumbnail_url}
              <img src={track.thumbnail_url} alt="" loading="lazy" />
            {:else}
              <div class="thumb-fallback">♪</div>
            {/if}
          </div>

          <div class="row-meta">
            <span class="row-title">{track.title}</span>
            <span class="row-artist">{track.artist}</span>
          </div>

          <div class="row-badge">
            <span class="codec-tag">{track.codec.toUpperCase()}</span>
          </div>

          <span class="row-duration">{player.formatTime(track.duration_seconds * 1000)}</span>

          <div class="row-actions">
            <span
              class="queue-btn"
              onclick={(e) => handleAddToQueue(e, track)}
              onkeydown={(e) => {
                if (e.key === 'Enter') handleAddToQueue(e as any, track);
              }}
              role="button"
              tabindex="0"
              title="Add to Playback Queue"
              aria-label="Add to Queue"
            >
              + QUEUE
            </span>
          </div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .echo-search-results {
    display: flex;
    flex-direction: column;
    gap: var(--echo-spacing-lg);
  }

  .results-header .title-text {
    font-family: "NDot57", monospace;
    font-size: 20px;
    letter-spacing: 0.1em;
    margin: 0;
    color: #fff;
  }

  .results-header .subtitle-text {
    font-family: "NDot57", monospace;
    font-size: 10px;
    color: var(--echo-text-muted);
    letter-spacing: 0.08em;
  }

  .back-link {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: var(--echo-radius-full);
    color: var(--echo-text-secondary);
    font-family: "NDot57", monospace;
    font-size: 10px;
    padding: 6px 14px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .back-link:hover {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.3);
  }

  .results-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .track-row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 16px;
    border-radius: var(--echo-radius-md);
    background: rgba(12, 13, 17, 0.45);
    border: 1px solid rgba(255, 255, 255, 0.04);
    transition: all 0.15s ease;
    cursor: pointer;
  }

  .track-row:hover {
    background: rgba(255, 255, 255, 0.06);
    border-color: rgba(255, 255, 255, 0.12);
    transform: translateX(3px);
  }

  .row-index {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-muted);
    width: 24px;
  }

  .row-thumb {
    width: 40px;
    height: 40px;
    border-radius: var(--echo-radius-sm);
    overflow: hidden;
    background: #000;
    flex-shrink: 0;
  }

  .row-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-fallback {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--echo-text-muted);
    font-size: 14px;
  }

  .row-meta {
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
    width: 60px;
  }

  .codec-tag {
    font-family: "NDot57", monospace;
    font-size: 9px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--echo-lossless-gold);
    letter-spacing: 0.06em;
  }

  .row-duration {
    font-family: "NDot57", monospace;
    font-size: 11px;
    color: var(--echo-text-muted);
    width: 50px;
    text-align: right;
  }

  .row-actions {
    display: flex;
    align-items: center;
  }

  .queue-btn {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--echo-text-secondary);
    font-family: "NDot57", monospace;
    font-size: 9px;
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .queue-btn:hover {
    color: #fff;
    border-color: var(--echo-color-accent);
    background: rgba(215, 25, 32, 0.12);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 80px 0;
    color: var(--echo-text-muted);
  }

  .empty-glyph {
    font-size: 40px;
    margin-bottom: 12px;
  }

  .empty-title {
    font-family: "NDot57", monospace;
    font-size: 14px;
    letter-spacing: 0.1em;
    color: var(--echo-text-primary);
    margin: 0;
  }

  .empty-sub {
    font-size: 12px;
    margin-top: 6px;
  }
</style>
