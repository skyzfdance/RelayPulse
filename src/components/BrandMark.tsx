import { ThunderboltFilled } from '@ant-design/icons';
import styles from '../styles/App.module.css';

export function BrandMark({ compact = false }: { compact?: boolean }) {
  return (
    <span className={compact ? styles.brandMarkCompact : styles.brandMark} aria-hidden="true">
      <ThunderboltFilled />
    </span>
  );
}
