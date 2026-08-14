import { requestClient } from '#/api/request';

export function getPentRuntimePolicy() {
  return requestClient.get('/pent/settings/runtime-policy');
}

export function updatePentRuntimePolicy(data: Record<string, any>) {
  return requestClient.put('/pent/settings/runtime-policy', data);
}

export interface PentModelBinding {
  enabled: boolean;
  inherited?: boolean;
  max_output_tokens: number;
  model_config_id: number;
  model_role: string;
  project_id: string;
  temperature: number;
  timeout_seconds: number;
}

export function getPentModelBindings() {
  return requestClient.get<PentModelBinding[]>('/pent/settings/model-bindings');
}

export function updatePentModelBindings(bindings: PentModelBinding[]) {
  return requestClient.put('/pent/settings/model-bindings', { bindings });
}

export function getPentPlatformSection(section: string) {
  return requestClient.get<Record<string, any>>(`/pent/settings/sections/${section}`);
}

export function updatePentPlatformSection(section: string, data: Record<string, any>) {
  return requestClient.put<Record<string, any>>(`/pent/settings/sections/${section}`, data);
}

export interface InfoCollectProviderConfig {
  api_key?: string;
  base_url: string;
  configured: boolean;
  enabled: boolean;
  id: 'fofa' | 'quake' | 'shodan' | 'zoomeye';
}

export function getInfoCollectConfig() {
  return requestClient.get<InfoCollectProviderConfig[]>('/pent/infocollect/config');
}

export function updateInfoCollectConfig(providers: InfoCollectProviderConfig[]) {
  return requestClient.put<InfoCollectProviderConfig[]>('/pent/infocollect/config', { providers });
}

export interface RobotChannelConfig { app_id: string; configured: boolean; enabled: boolean; id: string; secret?: string }
export function getRobotSettings() { return requestClient.get<RobotChannelConfig[]>('/pent/settings/robots'); }
export function updateRobotSettings(channels: RobotChannelConfig[]) { return requestClient.put<RobotChannelConfig[]>('/pent/settings/robots', { channels }); }
export function createTerminalSession() { return requestClient.post<{ id: string }>('/pent/settings/terminal/sessions'); }
export function closeTerminalSession(id: string) { return requestClient.delete(`/pent/settings/terminal/sessions/${id}`); }
export function writeTerminalInput(id: string, data: string) { return requestClient.post(`/pent/settings/terminal/sessions/${id}/input`, { data }); }
export function readTerminalOutput(id: string, after: number) { return requestClient.get<{ chunks: Array<{ data: string; sequence: number }>; sequence: number }>(`/pent/settings/terminal/sessions/${id}/output`, { params: { after } }); }
