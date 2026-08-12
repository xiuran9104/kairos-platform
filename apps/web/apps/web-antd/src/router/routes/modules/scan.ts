import type { RouteRecordRaw } from 'vue-router';

const routes: RouteRecordRaw[] = [
  {
    path: '/scan',
    name: 'Scan',
    meta: { title: '安全扫描', icon: 'lucide:shield-check', order: 30 },
    children: [
      { path: '/asset-ops/physical-server', component: () => import('#/views/asset-ops/physical-server/index.vue'), name: 'ScanPhysicalServer', meta: { title: '物理服务器', icon: 'lucide:server' } },
      { path: '/asset-ops/network-device', component: () => import('#/views/asset-ops/network-device/index.vue'), name: 'ScanNetworkDevice', meta: { title: '网络设备', icon: 'lucide:network' } },
      {
        path: '/asset-ops/dashboard',
        component: () => import('#/views/asset-ops/dashboard/index.vue'),
        name: 'ScanDashboard',
        meta: { title: '仪表盘', icon: 'lucide:gauge' },
      },
      {
        path: '/asset-ops/assets',
        component: () => import('#/views/asset-ops/asset/index.vue'),
        name: 'ScanAsset',
        meta: { title: '资产管理', icon: 'lucide:hard-drive' },
      },
      {
        path: '/asset-ops/zone',
        component: () => import('#/views/asset-ops/zone/index.vue'),
        name: 'ScanZone',
        meta: { title: '网络区域', icon: 'lucide:globe' },
      },
      {
        path: '/asset-ops/scanner',
        component: () => import('#/views/asset-ops/scanner/index.vue'),
        name: 'ScanScanner',
        meta: { title: '扫描节点', icon: 'lucide:scan-search' },
      },
      {
        path: '/asset-ops/task',
        component: () => import('#/views/asset-ops/task/index.vue'),
        name: 'ScanTask',
        meta: { title: '扫描任务', icon: 'lucide:play' },
      },
      {
        path: '/asset-ops/risk',
        component: () => import('#/views/asset-ops/risk/index.vue'),
        name: 'ScanRisk',
        meta: { title: '风险管理', icon: 'lucide:alert-triangle' },
      },
    ],
  },
  {
    path: '/asset-ops/infra',
    name: 'ScanInfra',
    meta: { title: '基础设施', icon: 'lucide:building-2', order: 31 },
    children: [
      {
        path: '/asset-ops/infra/machine-room',
        alias: ['/asset-ops/machine-room', '/asset-ops/cloud/machine-room'],
        component: () => import('#/views/asset-ops/machine-room/index.vue'),
        name: 'ScanMachineRoom',
        meta: { title: '机房管理', icon: 'lucide:warehouse' },
      },
      {
        path: '/asset-ops/infra/cabinet',
        alias: '/asset-ops/cloud/cabinet',
        component: () => import('#/views/asset-ops/cabinet/index.vue'),
        name: 'ScanCabinet',
        meta: { title: '机柜布局', icon: 'lucide:columns-3' },
      },
      {
        path: '/asset-ops/security-product',
        component: () => import('#/views/asset-ops/security-product/index.vue'),
        name: 'ScanSecurityProduct',
        meta: { title: '安全设备', icon: 'lucide:shield' },
      },
      { path: '/asset-ops/storage-device', component: () => import('#/views/asset-ops/storage-device/index.vue'), name: 'ScanStorageDevice', meta: { title: '存储设备', icon: 'lucide:database' } },
    ],
  },
  {
    path: '/asset-ops/cloud',
    name: 'ScanCloud',
    meta: { title: '云平台管理', icon: 'lucide:cloud', order: 32 },
    children: [
      {
        path: '/asset-ops/service-provider',
        component: () => import('#/views/asset-ops/service-provider/index.vue'),
        name: 'ScanServiceProvider',
        meta: { title: '服务商', icon: 'lucide:briefcase-business' },
      },
      {
        path: '/asset-ops/cloud-zone',
        component: () => import('#/views/asset-ops/cloud-platform/zone.vue'),
        name: 'ScanCloudZone',
        meta: { title: '云资源区', icon: 'lucide:map-pin' },
      },
      {
        path: '/asset-ops/cloud-platform',
        component: () => import('#/views/asset-ops/cloud-platform/platform.vue'),
        name: 'ScanCloudPlatform',
        meta: { title: '云平台', icon: 'lucide:cloud-cog' },
      },
      {
        path: '/asset-ops/cloud-provider-config',
        component: () => import('#/views/asset-ops/cloud-provider-config/index.vue'),
        name: 'ScanCloudProviderConfig',
        meta: { title: '云厂商对接', icon: 'lucide:plug-zap' },
      },
      {
        path: '/asset-ops/cloud-service-asset',
        component: () => import('#/views/asset-ops/cloud-service-asset/index.vue'),
        name: 'ScanCloudServiceAsset',
        meta: { title: '云资源实例', icon: 'lucide:server' },
      },
    ],
  },
  {
    path: '/asset-ops/biz',
    name: 'ScanBiz',
    meta: { title: '业务工单', icon: 'lucide:clipboard-list', order: 33 },
    children: [
      {
        path: '/asset-ops/business-application',
        component: () => import('#/views/asset-ops/business-application/index.vue'),
        name: 'ScanBusinessApplication',
        meta: { title: '业务应用', icon: 'lucide:layout-grid' },
      },
      {
        path: '/asset-ops/business-resource',
        component: () => import('#/views/asset-ops/business-resource/index.vue'),
        name: 'ScanBusinessResource',
        meta: { title: '业务资源', icon: 'lucide:package' },
      },
      {
        path: '/asset-ops/resource-ticket',
        component: () => import('#/views/asset-ops/resource-ticket/index.vue'),
        name: 'ScanResourceTicket',
        meta: { title: '资源工单', icon: 'lucide:ticket' },
      },
    ],
  },
];

export default routes;
