import eslint from '@eslint/js';
import prettier from 'eslint-config-prettier';
import perfectionist from 'eslint-plugin-perfectionist';
import pluginVue from 'eslint-plugin-vue';
import globals from 'globals';
import tseslint from 'typescript-eslint';

// 前端源码、测试和工具配置共用本工程的检查规则。
export default tseslint.config(
  {
    ignores: ['**/node_modules/**', 'dist/**', 'playwright-report/**', 'test-results/**'],
  },
  {
    linterOptions: {
      reportUnusedDisableDirectives: 'error',
    },
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      globals: {
        ...globals.browser,
      },
    },
    plugins: {
      perfectionist,
    },
    rules: {
      // 状态映射一律用查表（Record/Map），嵌套三元可读性太差。
      'no-nested-ternary': 'error',
      'vue/multi-word-component-names': ['error', { ignores: ['App'] }],
      // 属性折行交给 prettier（eslint-config-prettier 唯独漏掉这条），避免与格式化器打架。
      'vue/first-attribute-linebreak': 'off',
      // import 顺序：独立 import type 置顶 → npm 包 → 自己写的 @/ → 相对路径 → *.vue 置底。
      // 副作用导入（如 CSS）默认不参与排序（sortSideEffects 默认 false），彼此相对顺序保持不变。
      'perfectionist/sort-imports': [
        'error',
        {
          type: 'natural',
          order: 'asc',
          newlinesBetween: 1,
          internalPattern: ['^@/.*'],
          customGroups: [{ groupName: 'vue-sfc', elementNamePattern: '\\.vue$' }],
          groups: [
            'type',
            ['builtin', 'external'],
            'internal',
            ['parent', 'sibling', 'index'],
            'vue-sfc',
            'side-effect',
            'unknown',
          ],
        },
      ],
      // 花括号内：type 成员排在 value 成员之前（不强制拆成独立 import type）。
      'perfectionist/sort-named-imports': [
        'error',
        { type: 'natural', order: 'asc', groups: ['type-import', 'value-import'] },
      ],
    },
  },
  {
    files: ['*.{js,mjs,cjs,ts}'],
    languageOptions: { globals: globals.node },
  },
  prettier,
);
