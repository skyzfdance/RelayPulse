import type { RequestParameter } from '@/types/domain/sites';

export interface RequestParameterDraft extends RequestParameter {
  id: string;
}

export interface RequestPreview {
  method: string;
  address: string;
  headers: {
    configured: Record<string, string>;
    defaults: Record<string, string>;
  };
  body: Record<string, unknown>;
}

export interface RequestParameterValidation {
  key: string | null;
  value: string | null;
}

export type RequestParameterValidator = (
  parameters: RequestParameter[],
) => RequestParameterValidation[];
