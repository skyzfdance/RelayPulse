<script setup lang="ts">
import { useAppInfo } from '@/composables/useAppInfo';
import { useMenuRoutes } from '@/composables/useMenuRoutes';

import AppIcon from '@/components/AppIcon.vue';

const { appName, appVersion } = useAppInfo();
const { menuItems, selectedKeys, handleMenuClick } = useMenuRoutes();
</script>

<template>
  <a-layout-sider :width="216" theme="light">
    <div class="flex h-full flex-col border-e border-rp-line bg-rp-subtle px-3 pb-4.5">
      <!-- 顶部：logo 区（品牌徽标 + 应用名），左对齐 -->
      <div
        class="-mx-3 mb-4 flex min-h-19 flex-none items-center gap-2.5 border-b border-rp-line px-6 text-rp"
      >
        <span
          class="grid size-6.5 flex-none place-items-center rounded-md bg-rp-primary text-[13px] text-white"
          aria-hidden="true"
        >
          <AppIcon name="ThunderboltFilled" />
        </span>
        <strong>{{ appName }}</strong>
      </div>

      <!-- 中间：菜单占满剩余高度并自行滚动；背景透明以保持侧边栏底色一致 -->
      <a-menu
        class="min-h-0 flex-1 overflow-y-auto border-e-0! bg-transparent! px-2 py-1 [&_.ant-menu-item-group-title]:text-xs [&_.ant-menu-item-group-title]:text-rp-tertiary"
        mode="inline"
        :items="menuItems"
        :selected-keys="selectedKeys"
        @click="handleMenuClick"
      />

      <!-- 底部：帮助外链 + 版本号，左对齐（链接色随 antd 渲染为主色蓝） -->
      <div class="flex-none">
        <a
          class="mb-1 flex h-10 w-full items-center gap-3 rounded-md px-3 text-left text-sm text-rp-secondary hover:bg-rp-hover [&_.anticon]:w-4 [&_.anticon]:text-base"
          href="https://github.com/skyzfdance/RelayPulse/issues"
          target="_blank"
          rel="noreferrer"
          title="打开 GitHub Issues"
        >
          <AppIcon name="QuestionCircleOutlined" />
          帮助与反馈
        </a>
        <div class="px-3 pt-3 text-[11px] text-rp-quaternary">{{ appName }} {{ appVersion }}</div>
      </div>
    </div>
  </a-layout-sider>
</template>
