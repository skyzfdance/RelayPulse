import { runTauri } from './tauri-environment.mjs';

runTauri(['dev', ...process.argv.slice(2)]);
