import { requestClient } from '#/api/request';

import type { PentSkill } from './index';

export function updatePentSkill(
  id: string,
  data: {
    category?: string;
    content?: string;
    description?: string;
    name?: string;
    risk_level?: string;
    title?: string;
  },
) {
  return requestClient.put<PentSkill>(`/pent/skills/${id}`, data);
}

export interface PentSkillStatItem {
  skill_name: string;
  total_calls: number;
  success_calls: number;
  failed_calls: number;
  last_call_time?: string;
}

export interface PentSkillStats {
  total_skills: number;
  total_calls: number;
  total_success: number;
  total_failed: number;
  stats: PentSkillStatItem[];
}

export function getPentSkillStats() {
  return requestClient.get<PentSkillStats>('/pent/skills/stats');
}

export function clearPentSkillStats() {
  return requestClient.delete('/pent/skills/stats');
}
