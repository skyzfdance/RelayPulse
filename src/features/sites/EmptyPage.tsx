import { FileAddOutlined, PlusOutlined } from '@ant-design/icons';
import { Button, Empty } from 'antd';
import { navigateToView } from '../../app/view';
import styles from '../../styles/App.module.css';

export function EmptyPage() {
  return (
    <>
      <header className={styles.pageHeader}>
        <div>
          <h1>站点管理</h1>
          <p>添加并管理需要持续探测的模型站点</p>
        </div>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => navigateToView('add-site')}>
          添加站点
        </Button>
      </header>
      <div className={styles.emptyArea}>
        <Empty
          image={<FileAddOutlined className={styles.emptyIcon} />}
          description={
            <div className={styles.emptyCopy}>
              <h2>还没有站点</h2>
              <p>
                先添加一个 OpenAI、Anthropic、Gemini
                或兼容协议站点，连接成功后即可选择需要探测的模型。
              </p>
            </div>
          }
        >
          <Button type="primary" icon={<PlusOutlined />} onClick={() => navigateToView('add-site')}>
            添加第一个站点
          </Button>
        </Empty>
      </div>
    </>
  );
}
