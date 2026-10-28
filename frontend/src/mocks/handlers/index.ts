/**
 * mock 命令注册入口。
 *
 * callBackend 在浏览器模式下动态 import 本模块完成注册。
 * Tauri 模式直接调用后端命令，不加载此注册入口。
 */
import './catalog';
import './config/general';
import './config/notifications';
import './config/sites';
import './panel';
import './run/records';
