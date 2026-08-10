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

  const reload = useCallback(async () => {
    try {
      const next = await dataSource.getSnapshot();
      setSnapshot(next);
      setLoadError(null);
      return next;
    } catch (cause) {
      setLoadError(cause instanceof Error ? cause.message : '智力天梯数据加载失败');
      return null;
    } finally {
      setLoading(false);
    }
  }, [dataSource]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;

    const loadSnapshot = () => {
      void dataSource.getSnapshot().then(
        (next) => {
          if (disposed) return;
          setSnapshot(next);
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

    loadSnapshot();
    if (dataSource.subscribeToChanges) {
      void dataSource
        .subscribeToChanges(() => {
          if (!disposed) loadSnapshot();
        })
        .then((stop) => {
          if (disposed) stop();
          else unsubscribe = stop;
        });
    }
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [dataSource]);

  const refresh = useCallback(async () => {
    const result = await dataSource.refresh();
    await reload();
    return result;
  }, [dataSource, reload]);

  const updateSettings = useCallback(
    async (settings: IntelligenceAutoFetchSettings) => {
      const next = await dataSource.updateAutoFetchSettings(settings);
      setSnapshot(next);
      setLoadError(null);
      return next;
    },
    [dataSource],
  );

  return { snapshot, loading, loadError, refresh, reload, updateSettings };
}
