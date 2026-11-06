<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { player } from '../stores/playerState.svelte';

  let iframeElement: HTMLIFrameElement;
  let playerReady = $state(false);
  let pollInterval: any = null;

  // Track previous playback state to synchronize with Svelte store
  let lastTrackId: string | null = null;

  function loadYouTubeIframeApi(): Promise<void> {
    return new Promise((resolve) => {
      const win = window as any;
      if (win.YT && win.YT.Player) {
        resolve();
        return;
      }
      const tag = document.createElement('script');
      tag.src = 'https://www.youtube.com/iframe_api';
      const firstScriptTag = document.getElementsByTagName('script')[0];
      firstScriptTag?.parentNode?.insertBefore(tag, firstScriptTag);
      win.onYouTubeIframeAPIReady = () => resolve();
    });
  }

  let ytPlayerInstance: any = null;

  onMount(async () => {
    await loadYouTubeIframeApi();

    const win = window as any;
    ytPlayerInstance = new win.YT.Player('echo-hidden-audio-player', {
      height: '1',
      width: '1',
      playerVars: {
        autoplay: 0,
        controls: 0,
        disablekb: 1,
        fs: 0,
        modestbranding: 1,
        rel: 0,
        playsinline: 1,
        origin: window.location.origin,
      },
      events: {
        onReady: () => {
          playerReady = true;
          ytPlayerInstance.setVolume(Math.round(player.volume * 100));
        },
        onStateChange: (event: any) => {
          // YT.PlayerState: 1 = PLAYING, 2 = PAUSED, 0 = ENDED, 3 = BUFFERING
          if (event.data === 1) {
            player.isPlaying = true;
            player.isBuffering = false;
          } else if (event.data === 2) {
            player.isPlaying = false;
            player.isBuffering = false;
          } else if (event.data === 3) {
            player.isBuffering = true;
          } else if (event.data === 0) {
            player.playNext();
          }
        },
      },
    });

    // 10Hz progress synchronization loop
    pollInterval = setInterval(() => {
      if (!ytPlayerInstance || typeof ytPlayerInstance.getCurrentTime !== 'function') return;
      try {
        if (player.isPlaying) {
          const currentTimeSec = ytPlayerInstance.getCurrentTime();
          const durationSec = ytPlayerInstance.getDuration();
          if (currentTimeSec >= 0) {
            player.positionMs = Math.round(currentTimeSec * 1000);
          }
          if (durationSec > 0 && player.durationMs === 0) {
            player.durationMs = Math.round(durationSec * 1000);
          }
        }
      } catch (e) {}
    }, 100);
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
    if (ytPlayerInstance && typeof ytPlayerInstance.destroy === 'function') {
      ytPlayerInstance.destroy();
    }
  });

  // Reactively track song changes from the Svelte player store
  $effect(() => {
    const track = player.currentTrack;
    if (!track || !playerReady || !ytPlayerInstance) return;

    if (track.id !== lastTrackId) {
      lastTrackId = track.id;
      // Load and immediately play the new YouTube song
      if (typeof ytPlayerInstance.loadVideoById === 'function') {
        ytPlayerInstance.loadVideoById(track.id);
        player.isPlaying = true;
      }
    }
  });

  // Reactively track play/pause button clicks
  $effect(() => {
    if (!playerReady || !ytPlayerInstance) return;
    try {
      const state = ytPlayerInstance.getPlayerState();
      if (player.isPlaying && state !== 1) {
        ytPlayerInstance.playVideo();
      } else if (!player.isPlaying && state === 1) {
        ytPlayerInstance.pauseVideo();
      }
    } catch (e) {}
  });

  // Reactively track volume changes
  $effect(() => {
    if (!playerReady || !ytPlayerInstance) return;
    try {
      ytPlayerInstance.setVolume(Math.round(player.volume * 100));
    } catch (e) {}
  });
</script>

<!-- Completely invisible background audio host -->
<div class="hidden-audio-frame-host" aria-hidden="true">
  <div id="echo-hidden-audio-player"></div>
</div>

<style>
  .hidden-audio-frame-host {
    position: absolute;
    width: 1px;
    height: 1px;
    top: -9999px;
    left: -9999px;
    opacity: 0;
    pointer-events: none;
    overflow: hidden;
  }
</style>
