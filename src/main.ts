import { mount } from 'svelte';
import App from './App.svelte';
import './styles/tokens.css';

const target = document.getElementById('app');
if (!target) throw new Error('missing #app');

mount(App, { target });
