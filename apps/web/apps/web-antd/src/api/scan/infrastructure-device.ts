import { requestClient } from '#/api/request';
export interface InfrastructureDevice { id?:number; device_type:'network'|'storage'; name:string; category:string; vendor:string; model:string; serial_number?:string; management_ip?:string; business_ip?:string; machine_room_id?:number; rack_position?:string; status:string; specification?:string; capacity?:string; contact_person?:string; contact_phone?:string; remarks?:string; }
export function getInfrastructureDeviceList(deviceType:string){return requestClient.get<InfrastructureDevice[]>('/asset-ops/infrastructure-devices',{params:{device_type:deviceType}});}
export function createInfrastructureDevice(data:InfrastructureDevice){return requestClient.post('/asset-ops/infrastructure-devices',data);}
export function updateInfrastructureDevice(id:number,data:InfrastructureDevice){return requestClient.put(`/asset-ops/infrastructure-devices/${id}`,data);}
export function deleteInfrastructureDevice(id:number){return requestClient.delete(`/asset-ops/infrastructure-devices/${id}`);}
