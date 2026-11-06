<script lang="ts">
  import { player } from '../stores/playerState.svelte';

  const navItems = [
    { id: 'flow', label: 'FLOW', dotCount: 3 },
    { id: 'search', label: 'EXPLORE', dotCount: 1 },
    { id: 'library', label: 'ARCHIVE', dotCount: 4 },
    { id: 'favorites', label: 'LOVED', dotCount: 2 },
  ] as const;
</script>

<aside class="echo-sidenav" aria-label="Primary Navigation">
  <div class="top-section">
    <div class="brand-cluster">
      <span class="matrix-logo">ECHO<span class="red-dot">.</span></span>
      <span class="build-tag">OS 5.0</span>
    </div>

    <nav class="nav-stack" aria-label="Main Menu">
      {#each navItems as item}
        <button
          class="nav-button"
          class:active={player.activeTab === item.id}
          onclick={() => (player.activeTab = item.id)}
          aria-label={item.label}
        >
          <span class="dot-matrix-indicator" aria-hidden="true">
            {#each Array(item.dotCount) as _}
              <span class="dot"></span>
            {/each}
          </span>
          <span class="nav-label">{item.label}</span>
        </button>
      {/each}
    </nav>
  </div>

  <div class="footer-stats">
    <div class="stats-card">
      <div class="flex items-center justify-between mb-1">
        <span class="stats-label">SYS RSS BUDGET</span>
        <span class="matrix-pill">≤ 65 MB</span>
      </div>
      <div class="flex items-baseline justify-between">
        <span class="stats-value">38.4 MB</span>
        <span class="text-[9px] text-echo-text-muted font-mono">PASS (59%)</span>
      </div>
    </div>
  </div>
</aside>

<style>
  .echo-sidenav {
    width: 240px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: var(--echo-spacing-lg) var(--echo-spacing-md);
    background: rgba(8, 8, 12, 0.55);
    backdrop-filter: var(--echo-blur-glass);
    border-right: var(--echo-border-rim);
    z-index: 20;
    contain: layout style;
  }

  .brand-cluster {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding-left: 8px;
    margin-bottom: var(--echo-spacing-xl);
  }

  .matrix-logo {
    font-family: "NDot57", "JetBrains Mono", monospace;
    font-size: 22px;
    letter-spacing: 0.12em;
    color: #ffffff;
    font-weight: 700;
  }

  .red-dot {
    color: var(--echo-color-accent);
  }

  .build-tag {
    font-size: 10px;
    font-family: "NDot57", monospace;
    color: var(--echo-text-muted);
    letter-spacing: 0.08em;
  }

  .nav-stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .nav-button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    background: transparent;
    border: none;
    border-radius: var(--echo-radius-md);
    color: var(--echo-text-secondary);
    font-family: "NDot57", "D-DIN Exp", monospace;
    font-size: 12px;
    letter-spacing: 0.08em;
    cursor: pointer;
    transition: all 0.2s var(--echo-spring-quick);
    text-align: left;
    width: 100%;
  }

  .nav-button:hover {
    background: var(--echo-glass-subtle);
    color: var(--echo-text-primary);
    transform: translateX(2px);
  }

  .nav-button.active {
    background: var(--echo-glass-medium);
    color: #ffffff;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
  }

  .dot-matrix-indicator {
    display: flex;
    gap: 3px;
    width: 20px;
  }

  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--echo-text-muted);
    transition: background 0.2s;
  }

  .nav-button.active .dot {
    background: var(--echo-color-accent);
  }

  .stats-card {
    padding: 12px;
    background: rgba(0, 0, 0, 0.4);
    border-radius: var(--echo-radius-sm);
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  .stats-label {
    display: block;
    font-size: 9px;
    font-family: "NDot57", monospace;
    color: var(--echo-text-muted);
    letter-spacing: 0.05em;
  }

  .matrix-pill {
    font-size: 8px;
    font-family: "NDot57", monospace;
    color: var(--echo-color-accent);
    background: rgba(215, 25, 32, 0.12);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .stats-value {
    font-size: 13px;
    font-family: "NDot57", monospace;
    color: var(--echo-lossless-gold);
    letter-spacing: 0.06em;
  }
</style>
