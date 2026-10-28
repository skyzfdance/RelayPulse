import { createPinia } from 'pinia';
import { createApp } from 'vue';

import { vLoading } from './directives/loading';
import { setupI18n } from './i18n';
import { router } from './router';

import App from './App.vue';

import 'antdv-next/dist/reset.css';
import './styles.css';

setupI18n();

const app = createApp(App);

app.use(createPinia());
app.use(router);
app.directive('loading', vLoading);

// 各页面在自己的生命周期里按需调用所属业务的 API。
app.mount('#app');
