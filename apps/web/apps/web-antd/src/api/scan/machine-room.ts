import { requestClient } from '#/api/request';

export namespace ScanMachineRoomApi {
  export interface MachineRoom {
    id?: number; room_name: string; room_code: string;
    facility_type: string; address: string; platform_id: number;
    room_type: string; contact_person: string; contact_phone: string;
    floor?: string; cabinet_count?: number; area_size?: string;
    remarks?: string; status: string; created_at?: string; updated_at?: string;
  }
}
export function getMachineRoomList() { return requestClient.get<ScanMachineRoomApi.MachineRoom[]>('/asset-ops/machine-rooms'); }
export function getMachineRoom(id: number) { return requestClient.get<ScanMachineRoomApi.MachineRoom>(`/asset-ops/machine-rooms/${id}`); }
export function createMachineRoom(data: ScanMachineRoomApi.MachineRoom) { return requestClient.post('/asset-ops/machine-rooms', data); }
export function updateMachineRoom(id: number, data: ScanMachineRoomApi.MachineRoom) { return requestClient.put(`/asset-ops/machine-rooms/${id}`, data); }
export function deleteMachineRoom(id: number) { return requestClient.delete(`/asset-ops/machine-rooms/${id}`); }
