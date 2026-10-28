import { defineStore } from 'pinia';
import { ref } from 'vue';

export const useDemoStore = defineStore('vue-demo', () => {
  const pingCount = ref(0);

  function increment() {
    pingCount.value += 1;
  }

  return { pingCount, increment };
});
