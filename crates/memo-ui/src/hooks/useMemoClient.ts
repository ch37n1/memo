import { invoke } from '@tauri-apps/api/core';

export type MountMode = 'read_only' | 'read_write';
export type Audience = 'private' | 'shared';

export interface MountDto {
  name: string;
  root_path: string;
  mode: MountMode;
  audience: Audience;
  description?: string | null;
  policy: {
    hide_globs: string[];
    deny_read_globs: string[];
    deny_write_globs: string[];
    max_read_bytes?: number | null;
    max_write_bytes?: number | null;
  };
  created_at: string;
  updated_at: string;
}

export interface TokenViewDto {
  id: string;
  name: string;
  scopes: string[];
  created_at: string;
  expires_at: { Never?: never } | { At: string };
  last_used_at?: string | null;
}

export interface CreatedTokenDto {
  id: string;
  name: string;
  token: string;
  scopes: string[];
  created_at: string;
  expires_at: { Never?: never } | { At: string };
}

export interface AuditEntryDto {
  id: number;
  timestamp: string;
  token_id?: string | null;
  operation: string;
  mount?: string | null;
  path?: string | null;
  result: 'ok' | 'error';
  error_code?: string | null;
}

export interface TreeNodeDto {
  name: string;
  kind: 'file' | 'dir';
  size?: number | null;
  modified_at?: string | null;
  children: TreeNodeDto[];
}

export interface TreeResponseDto {
  path: string;
  depth: number;
  truncated: boolean;
  tree: TreeNodeDto;
}

export interface CreateMountInput {
  name: string;
  rootPath: string;
  mode: MountMode;
  audience: Audience;
  description?: string;
}

export interface CreateTokenInput {
  name: string;
  scopes: string[];
  expiresAt?: string;
}

export interface AuditFilterInput {
  mount?: string;
  operation?: string;
  result?: 'ok' | 'error';
  limit?: number;
}

export function useMemoClient() {
  return {
    health: (baseUrl: string) => invoke<{ status: string; version: string }>('health', { baseUrl }),

    listMounts: (baseUrl: string, token: string) =>
      invoke<MountDto[]>('list_mounts', { baseUrl, token }),

    createMount: (baseUrl: string, token: string, input: CreateMountInput) =>
      invoke<MountDto>('create_mount', { baseUrl, token, input }),

    removeMount: (baseUrl: string, token: string, name: string) =>
      invoke<{ removed: boolean }>('remove_mount', { baseUrl, token, name }),

    listTokens: (baseUrl: string, token: string) =>
      invoke<TokenViewDto[]>('list_tokens', { baseUrl, token }),

    createToken: (baseUrl: string, token: string, input: CreateTokenInput) =>
      invoke<CreatedTokenDto>('create_token', { baseUrl, token, input }),

    revokeToken: (baseUrl: string, token: string, id: string) =>
      invoke<{ revoked: boolean }>('revoke_token', { baseUrl, token, id }),

    queryAudit: (baseUrl: string, token: string, filter: AuditFilterInput) =>
      invoke<AuditEntryDto[]>('query_audit', { baseUrl, token, filter }),

    browseTree: (baseUrl: string, token: string, mount: string, depth: number) =>
      invoke<TreeResponseDto>('browse_tree', { baseUrl, token, mount, depth }),
  };
}
