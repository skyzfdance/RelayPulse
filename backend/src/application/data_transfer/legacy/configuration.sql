-- 通用设置。
-- SQLite 使用 INTEGER + CHECK 表达严格的布尔值。
CREATE TABLE general_settings (
  -- 单例主键：固定为 1，整张表只保存一份通用设置；不是开关。
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  -- 开机自动启动：0 = 关闭，1 = 开启；默认关闭。
  launch_at_startup INTEGER NOT NULL DEFAULT 0 CHECK (launch_at_startup IN (0, 1)),
  -- 关闭主窗口后继续在后台运行：0 = 不继续，1 = 继续；默认继续。
  -- 同时作为自动探测总开关，主窗口打开时也生效；关闭仅停止新派发，不中断已执行的请求。
  continue_in_background INTEGER NOT NULL DEFAULT 1 CHECK (continue_in_background IN (0, 1)),
  -- 显示系统托盘/菜单栏图标：0 = 隐藏，1 = 显示；默认显示。
  show_tray_icon INTEGER NOT NULL DEFAULT 1 CHECK (show_tray_icon IN (0, 1)),
  -- 界面主题：system = 跟随系统，light = 浅色，dark = 深色；默认跟随系统。
  theme TEXT NOT NULL DEFAULT 'system' CHECK (theme IN ('system', 'light', 'dark')),
  -- 界面语言：zh-CN = 简体中文；当前仅支持这一种语言。
  language TEXT NOT NULL DEFAULT 'zh-CN' CHECK (language = 'zh-CN'),
  -- 探测历史记录保留天数：1–3650 天，默认 30 天。
  history_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (history_retention_days BETWEEN 1 AND 3650)
) STRICT;

INSERT INTO general_settings (singleton) VALUES (1);
