import { ThunderboltFilled } from '@ant-design/icons';

export function BrandMark({ compact = false }: { compact?: boolean }) {
  return (
    <span
      className={`grid flex-none place-items-center bg-rp-primary text-white ${
        compact ? 'size-5.5 rounded-[5px] text-[11px]' : 'size-6.5 rounded-md text-[13px]'
      }`}
      aria-hidden="true"
    >
      <ThunderboltFilled />
    </span>
  );
}
