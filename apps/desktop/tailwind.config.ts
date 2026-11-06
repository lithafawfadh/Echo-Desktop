import type { Config } from 'tailwindcss';

export default {
  content: ['./index.html', './src/**/*.{svelte,js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        'echo-ground': '#060608',
        'echo-surface-base': '#0c0d11',
        'echo-surface-elevated': '#13151b',
        'echo-surface-floating': '#1a1c24',
        'echo-accent': '#d71920',
        'echo-accent-glow': 'rgba(215, 25, 32, 0.35)',
        'echo-lossless-gold': '#d4af37',
        'echo-lossless-hi-res': '#e5a93c',
        'echo-text-primary': '#f2f3f5',
        'echo-text-secondary': '#90949d',
        'echo-text-muted': '#545761',
      },
      fontFamily: {
        matrix: ['NDot57', 'D-DIN Exp', 'JetBrains Mono', 'monospace'],
        display: ['-apple-system', 'BlinkMacSystemFont', 'SF Pro Display', 'Inter', 'sans-serif'],
      },
      borderRadius: {
        'echo-sm': '8px',
        'echo-md': '14px',
        'echo-lg': '20px',
        'echo-xl': '24px',
      },
      backdropBlur: {
        glass: '24px',
        ambient: '64px',
      },
    },
  },
  plugins: [],
} satisfies Config;
