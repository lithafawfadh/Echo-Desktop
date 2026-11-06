import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');
if (!target) {
  throw new Error('Root #app container not found in DOM');
}

const app = mount(App, {
  target,
});

export default app;
