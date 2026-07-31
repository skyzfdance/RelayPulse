import { FileAddOutlined, PlusOutlined } from '@ant-design/icons';
import { Button, Empty } from 'antd';
import { navigateToView } from '../../app/view';
import { pageHeaderClass } from '../../styles/tailwind';

export function EmptyPage() {
  return (
    <>
      <header className={pageHeaderClass}>
        <div>
          <h1>站点管理</h1>
          <p>添加并管理需要持续探测的模型站点</p>
        </div>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => navigateToView('add-site')}>
          添加站点
        </Button>
      </header>
      <div className="grid flex-1 place-items-center pb-10">
        <Empty
          image={<FileAddOutlined className="text-7xl text-rp-quaternary" />}
          description={
            <div className="max-w-130">
              <h2 className="mt-2 mb-1 text-lg text-rp">还没有站点</h2>
              <p className="m-0 leading-5.5 text-rp-secondary">
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
