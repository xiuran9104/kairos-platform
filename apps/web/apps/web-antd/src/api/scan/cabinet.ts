import { requestClient } from '#/api/request';

export interface CabinetRow {
  id?: number;
  machine_room_id: number;
  row_name: string;
  row_code: string;
  sort?: number;
  status: string;
  remarks?: string;
}

export interface Cabinet {
  id?: number;
  row_id: number;
  cabinet_name: string;
  cabinet_code: string;
  total_u: number;
  power_capacity_kw?: number;
  status: string;
  remarks?: string;
}

export const getCabinetRowList = (machineRoomId?: number) =>
  requestClient.get<CabinetRow[]>('/asset-ops/cabinet-rows', { params: { machine_room_id: machineRoomId } });
export const createCabinetRow = (data: CabinetRow) => requestClient.post('/asset-ops/cabinet-rows', data);
export const updateCabinetRow = (id: number, data: CabinetRow) => requestClient.put(`/asset-ops/cabinet-rows/${id}`, data);
export const deleteCabinetRow = (id: number) => requestClient.delete(`/asset-ops/cabinet-rows/${id}`);

export const getCabinetList = (rowId?: number) =>
  requestClient.get<Cabinet[]>('/asset-ops/cabinets', { params: { row_id: rowId } });
export const createCabinet = (data: Cabinet) => requestClient.post('/asset-ops/cabinets', data);
export const updateCabinet = (id: number, data: Cabinet) => requestClient.put(`/asset-ops/cabinets/${id}`, data);
export const deleteCabinet = (id: number) => requestClient.delete(`/asset-ops/cabinets/${id}`);
export const configureMachineRoomCabinets = (data: { machine_room_id: number; row_count: number; cabinets_per_row: number; cabinet_total_u: number }) =>
  requestClient.post('/asset-ops/cabinets/configure-room', data);
