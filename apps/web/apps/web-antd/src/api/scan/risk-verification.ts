import { requestClient } from '#/api/request';

export namespace ScanRiskVerificationApi {
  export type Status =
    | 'approved'
    | 'completed'
    | 'failed'
    | 'pending_approval'
    | 'rejected'
    | 'running';

  export interface Verification {
    approved_at?: string;
    approved_by?: string;
    authorization_statement: string;
    created_at: string;
    error?: string;
    execution_id?: string;
    executed_at?: string;
    executed_by?: string;
    id: string;
    requested_by: string;
    rejected_at?: string;
    rejected_by?: string;
    result: Record<string, unknown>;
    risk_id: string;
    status: Status;
    target: string;
    tool_name: string;
    updated_at: string;
  }

  export interface CreateRequest {
    authorization_statement: string;
    target: string;
    tool_name:
      | 'command_injection_validate'
      | 'csrf_validate'
      | 'file_inclusion_validate'
      | 'file_upload_validate'
      | 'nuclei'
      | 'sqli_validate'
      | 'xss_validate';
  }
}

export function getRiskVerifications() {
  return requestClient.get<ScanRiskVerificationApi.Verification[]>(
    '/pent/scan-risk-verifications',
  );
}

export function createRiskVerification(
  riskId: string,
  data: ScanRiskVerificationApi.CreateRequest,
) {
  return requestClient.post(
    `/pent/scan-risks/${riskId}/verifications`,
    data,
  );
}

export function approveRiskVerification(id: string) {
  return requestClient.post(`/pent/scan-risk-verifications/${id}/approve`, {
    authorization_confirmed: true,
  });
}

export function rejectRiskVerification(id: string) {
  return requestClient.post(`/pent/scan-risk-verifications/${id}/reject`);
}

export function executeRiskVerification(id: string) {
  return requestClient.post(`/pent/scan-risk-verifications/${id}/execute`);
}
