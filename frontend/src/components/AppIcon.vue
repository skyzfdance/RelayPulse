<script setup lang="ts">
import type { Component } from 'vue';

import {
  AimOutlined,
  ApiOutlined,
  ArrowRightOutlined,
  BellOutlined,
  BookOutlined,
  ClockCircleOutlined,
  CodeOutlined,
  DeleteOutlined,
  DownOutlined,
  EditOutlined,
  ExperimentOutlined,
  ExportOutlined,
  FileAddOutlined,
  FileSearchOutlined,
  FolderOpenOutlined,
  HistoryOutlined,
  HolderOutlined,
  InfoCircleOutlined,
  LinkOutlined,
  MoreOutlined,
  PlayCircleOutlined,
  PlusOutlined,
  ProfileOutlined,
  QuestionCircleOutlined,
  ReloadOutlined,
  SendOutlined,
  SettingOutlined,
  SyncOutlined,
  ThunderboltFilled,
  TrophyOutlined,
} from '@antdv-next/icons';
import { computed } from 'vue';

const props = defineProps<{
  /** 已登记的图标名；新增图标时同步加入下方映射，未登记时渲染为空并告警。 */
  name?: string;
}>();

// 保留名称配置的用法，显式导入让构建工具能够移除未使用的图标。
const iconComponents = new Map<string, Component>(
  Object.entries({
    AimOutlined,
    ApiOutlined,
    ArrowRightOutlined,
    BellOutlined,
    BookOutlined,
    ClockCircleOutlined,
    CodeOutlined,
    DeleteOutlined,
    DownOutlined,
    EditOutlined,
    ExperimentOutlined,
    ExportOutlined,
    FileAddOutlined,
    FileSearchOutlined,
    FolderOpenOutlined,
    HistoryOutlined,
    HolderOutlined,
    InfoCircleOutlined,
    LinkOutlined,
    MoreOutlined,
    PlayCircleOutlined,
    PlusOutlined,
    ProfileOutlined,
    QuestionCircleOutlined,
    ReloadOutlined,
    SendOutlined,
    SettingOutlined,
    SyncOutlined,
    ThunderboltFilled,
    TrophyOutlined,
  }),
);

const iconComponent = computed(() => {
  if (!props.name) return undefined;
  const matched = iconComponents.get(props.name);
  if (!matched) {
    console.warn(`[AppIcon] 未登记图标组件：${props.name}（请检查 AppIcon 图标映射）`);
    return undefined;
  }
  return matched;
});
</script>

<template>
  <component :is="iconComponent" v-if="iconComponent" />
</template>
