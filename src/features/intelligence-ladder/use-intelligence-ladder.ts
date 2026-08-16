import { useCallback, useEffect, useState } from 'react';
import type {
  IntelligenceAutoFetchSettings,
  IntelligenceLadderDataSource,
  IntelligenceLadderSnapshot,
} from './intelligence-ladder-types';

export function useIntelligenceLadder(dataSource: IntelligenceLadderDataSource) {
  const [snapshot, setSnapshot] = useState<IntelligenceLadderSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const applySnapshot = useCallback((next: IntelligenceLadderSnapshot) => {
    setSnapshot((current) => (current && current.revision > next.revision ? current : next));
  }, []);

  const reload = useCallback(async () => {
    try {
      const next = await dataSource.getSnapshot();
      applySnapshot(next);
      setLoadError(null);
      return next;
    } catch (cause) {
      setLoadError(cause instanceof Error ? cause.message : '智力天梯数据加载失败');
      return null;
    } finally {
      setLoading(false);
    }
  }, [applySnapshot, dataSource]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;

    const loadSnapshot = () => {
      void dataSource.getSnapshot().then(
        (next) => {
          if (disposed) return;
          applySnapshot(next);
          setLoadError(null);
          setLoading(false);
        },
        (cause: unknown) => {
          if (disposed) return;
          setLoadError(cause instanceof Error ? cause.message : '智力天梯数据加载失败');
          setLoading(false);
        },
      );
    };

    if (dataSource.subscribeToChanges) {
      void dataSource
        .subscribeToChanges(() => {
          if (!disposed) loadSnapshot();
        })
        .then((stop) => {
          if (disposed) {
            stop();
            return;
          }
          unsubscribe = stop;
          loadSnapshot();
        })
        .catch(() => {
          if (!disposed) loadSnapshot();
        });
    } else {
      loadSnapshot();
    }
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [applySnapshot, dataSource]);

  const refresh = useCallback(async () => {
    const result = await dataSource.refresh();
    await reload();
    return result;
  }, [dataSource, reload]);

  const updateSettings = useCallback(
    async (settings: IntelligenceAutoFetchSettings) => {
      const next = await dataSource.updateAutoFetchSettings(settings);
      applySnapshot(next);
      setLoadError(null);
      return next;
    },
    [applySnapshot, dataSource],
  );

  return { snapshot, loading, loadError, refresh, reload, updateSettings };
}
