import { requestClient } from '#/api/request';

export namespace ToonflowApi {
  export interface Project {
    id: number;
    projectType: string;
    imageModel?: number;
    imageQuality: string;
    videoModel?: number;
    name: string;
    intro: string;
    type: string;
    artStyle: string;
    directorManual: string;
    mode: string;
    videoRatio: string;
    createTime: number;
    updateTime: number;
  }

  export interface SaveProject {
    id?: number;
    projectType: string;
    imageModel?: number;
    imageQuality: string;
    videoModel?: number;
    name: string;
    intro: string;
    type: string;
    artStyle: string;
    directorManual: string;
    mode: string;
    videoRatio: string;
  }

  export interface NovelChapter {
    id: number;
    index: number;
    reel: string;
    chapter: string;
    chapterData: string;
    projectId: number;
    eventState: number;
    event?: string;
    errorReason?: string;
    createTime: number;
  }

  export interface Script {
    id: number;
    name: string;
    content: string;
    projectId: number;
    extractState?: number;
    errorReason?: string;
    createTime: number;
    relatedAssets: Array<{ id: number; name: string }>;
  }

  export interface Asset {
    derive?: Asset[];
    appearances?: Array<{
      id: number;
      roleAssetId: number;
      name: string;
      scenes: string[];
      costumePrompt: string;
      description: string;
    }>;
    id: number;
    name: string;
    prompt: string;
    remark?: string;
    type: string;
    description: string;
    scriptId?: number;
    imageId?: number;
    imageFilePath?: string;
    imageState?: string;
    imageErrorReason?: string;
    parentAssetId?: number;
    appearanceId?: number;
    projectId: number;
    flowId?: number;
    audioBindState?: number;
  }

  export interface LibraryAsset extends Asset {
    linkedToProject: boolean;
    sourceProjectName: string;
  }

  export interface Storyboard {
    id: number;
    scriptId: number;
    projectId: number;
    prompt: string;
    duration?: number;
    state?: string;
    track?: string;
    videoDesc?: string;
    shouldGenerateImage: number;
    associateAssetsIds: number[];
    filePath?: string;
    flowId?: number;
    index?: number;
    reason?: string;
    src?: string;
    trackId?: number;
  }

  export interface AgentDeployment {
    id: number;
    key: string;
    description: string;
    name: string;
    temperature: number;
    maxOutputTokens: number;
    disabled: boolean;
    modelConfigId?: number;
    modelType: 'chat' | 'image' | 'speech' | 'video';
  }

  export interface ArtStyle { id: number; name: string; fileUrl: string; label: string; prompt: string; createTime: number }
  export interface Task { id: number; projectId?: number; projectName?: string; taskClass: string; relatedObjects: string; model: string; description: string; state: string; startTime?: number; reason?: string }
  export interface Prompt { id: number; name: string; type: string; data: string; useData?: string; sourceKey?: string }
  export interface Skill { id: string; name: string; description: string; type: string; path: string; state: number; createTime: number; updateTime: number }
  export interface ProjectStatistics { roleCount: number; scriptCount: number; videoCount: number; storyboardCount: number }
  export interface CreativeManual { id: number; kind: 'director' | 'visual'; name: string; path: string; images: string[]; data: Array<{ label: string; value: string; data: string }>; createTime: number; updateTime: number }
  export interface AgentMemory { id: number; role: string; content: string; memoryType: 'message' | 'summary'; createTime: number }
  export interface AgentRun { id: number; agentType: string; isolationKey: string; projectId: number; scriptId?: number; input: string; output?: string; state: string; errorReason?: string; startTime: number; finishTime?: number }
  export interface AgentRunEvent { id: number; runId: number; eventType: string; data: { text?: string }; createTime: number }
}

export const getArtStyles = () => requestClient.get<ToonflowApi.ArtStyle[]>('/toonflow/art-styles');
export const saveArtStyle = (data: Partial<ToonflowApi.ArtStyle> & { name: string }) => requestClient.post('/toonflow/art-styles', data);
export const deleteArtStyle = (id: number) => requestClient.delete(`/toonflow/art-styles/${id}`);
export const getTasks = () => requestClient.get<ToonflowApi.Task[]>('/toonflow/tasks');
export const getPrompts = () => requestClient.get<ToonflowApi.Prompt[]>('/toonflow/prompts');
export const savePrompt = (data: Partial<ToonflowApi.Prompt> & { name: string; type: string }) => requestClient.post('/toonflow/prompts', data);
export const deletePrompt = (id: number) => requestClient.delete(`/toonflow/prompts/${id}`);
export const getSkills = () => requestClient.get<ToonflowApi.Skill[]>('/toonflow/skills');
export const getSkillContent = (path: string) => requestClient.post<string>('/setting/skillManagement/getSkillContent', { path });
export const saveSkillContent = (path: string, content: string) => requestClient.post('/setting/skillManagement/saveSkillContent', { path, content });
export const getProjectStatistics = (projectId: number) => requestClient.post<ToonflowApi.ProjectStatistics>('/general/generalStatistics', { projectId });
export const updateProjectProfile = (data: { id: number; intro?: string; type?: string; artStyle?: string; videoRatio?: string; projectType?: string }) => requestClient.post('/general/updateProject', data);
export const getAgentModelDetails = (key: 'productionAgent' | 'scriptAgent') => requestClient.post<Record<string, any>>('/project/getModelDetails', { key });
export const generateNovelEvents = (projectId: number, novelIds: number[]) => requestClient.post('/novel/event/generateEvents', { projectId, novelIds, concurrentCount: 5 });
export const extractScriptAssets = (projectId: number, scriptIds: number[]) => requestClient.post('/script/extractAssets', { projectId, scriptIds, groupSize: 5 });
export const pollScriptAssets = (ids: number[]) => requestClient.post<Array<{ id: number; extractState: number; errorReason?: string; appearanceCount: number }>>('/script/pollScriptAssets', { ids });
export const polishAssetPrompt = (data: { assetsId: number; projectId: number; type: string; name: string; describe: string }) => requestClient.post<{ prompt: string; assetsId: number }>('/assetsGenerate/polishAssetsPrompt', data);
export const generateAssetImage = (data: { projectId: number; model: number | string; resolution: string; id: number; type: string; name: string; prompt: string; base64?: string }) => requestClient.post<{ path: string; assetsId: number }>('/assetsGenerate/generateAssets', data);
export const cancelAssetImage = (id: number) => requestClient.post('/assetsGenerate/cancelGenerate', { id });
export const uploadMaterial = (data: { projectId: number; base64Data: string; type?: string; name: string }) => requestClient.post('/assets/uploadClip', data);
export const getMaterialData = (projectId: number, scriptId?: number) => requestClient.post<{ data: any[]; video: any[] }>('/assets/getMaterialData', { projectId, scriptId });
export const generateFlowImage = (data: { projectId: number; model: number | string; quality: string; ratio: string; prompt: string; references?: string[]; targetType?: 'costume' | 'role' | 'scene' | 'storyboard' | 'tool' }) => requestClient.post<{ url: string }>('/production/editImage/generateFlowImage', data);
export const getImageFlow = (id: number) => requestClient.post<{ id: number; nodes: any[]; edges: any[] } | null>('/production/editImage/getImageFlow', { id });
export const saveImageFlow = (nodes: any[], edges: any[], assetId?: number) => requestClient.post<{ id: number }>('/production/editImage/saveImageFlow', { nodes, edges, assetId });
export const updateImageFlow = (flowId: number, nodes: any[], edges: any[]) => requestClient.post('/production/editImage/updateImageFlow', { flowId, nodes, edges });
export const uploadFlowImage = (projectId: number, scriptId: number, base64Data: string) => requestClient.post<string>('/production/editImage/uploadImage', { projectId, scriptId, base64Data });
export const generateStoryboardImages = (data: { storyboardIds: number[]; projectId: number; scriptId: number; concurrentCount?: number; compulsory?: boolean }) => requestClient.post('/production/storyboard/batchGenerateImage', data);
export const pollStoryboardImages = (ids: number[]) => requestClient.post<Array<Pick<ToonflowApi.Storyboard, 'filePath' | 'id' | 'prompt' | 'reason' | 'src' | 'state'>>>('/production/storyboard/pollingImage', { ids });
export const batchDeleteStoryboards = (ids: number[], projectId: number) => requestClient.post('/production/storyboard/batchDelete', { ids, projectId });
export const previewStoryboardImages = (storyboardIds: number[]) => requestClient.post<string | null>('/production/storyboard/previewImage', { storyboardIds });
export const downloadStoryboardPreview = (storyboardIds: number[]) => requestClient.download<Blob>('/production/storyboard/downPreviewImage', { method: 'POST', data: { storyboardIds } });
export const updateStoryboardUrl = (id: number, url: string, flowId: number) => requestClient.post('/production/storyboard/updateStoryboardUrl', { id, url, flowId });
export const getCreativeManuals = () => requestClient.get<ToonflowApi.CreativeManual[]>('/toonflow/manuals');
export const saveCreativeManual = (manual: Partial<ToonflowApi.CreativeManual> & { kind: 'director' | 'visual'; name: string; path: string }) => requestClient.post(`/project/${manual.kind === 'visual' ? (manual.id ? 'editVisualManual' : 'addVisualManual') : (manual.id ? 'editDirectorlManual' : 'addDirectorManual')}`, { name: manual.name, images: manual.images ?? [], data: manual.data ?? [], ...(manual.kind === 'visual' ? { stylePath: manual.path } : { directorManual: manual.path }) });
export const deleteCreativeManual = (kind: 'director' | 'visual', name: string) => requestClient.post(`/project/${kind === 'visual' ? 'deleteVisualManual' : 'deleteDirectorManual'}`, { name });

export function getProjects() {
  return requestClient.get<ToonflowApi.Project[]>('/toonflow/projects');
}

export function getProject(id: number) {
  return requestClient.get<ToonflowApi.Project>(`/toonflow/projects/${id}`);
}

export function createProject(data: ToonflowApi.SaveProject) {
  return requestClient.post<{ id: number }>('/toonflow/project/addProject', data);
}

export function updateProject(data: ToonflowApi.SaveProject) {
  return requestClient.post('/toonflow/project/editProject', data);
}

export function deleteProject(id: number) {
  return requestClient.post('/toonflow/project/delProject', { id });
}

export function addNovel(projectId: number, data: Array<Partial<ToonflowApi.NovelChapter>>) {
  return requestClient.post('/toonflow/novel/addNovel', { projectId, data });
}

export function getNovelData(projectId: number) {
  return requestClient.post<ToonflowApi.NovelChapter[]>('/toonflow/novel/getNovelData', {
    projectId,
  });
}

export function updateNovel(data: Partial<ToonflowApi.NovelChapter> & { id: number }) {
  return requestClient.post('/toonflow/novel/updateNovel', data);
}

export function deleteNovel(id: number) {
  return requestClient.post('/toonflow/novel/delNovel', { id });
}

export function getScripts(projectId: number, name = '') {
  return requestClient.post<ToonflowApi.Script[]>('/toonflow/script/getScrptApi', {
    projectId,
    name,
  });
}

export function addScript(data: {
  assets: number[];
  content: string;
  name: string;
  projectId: number;
  shouldGenerateImage?: number;
}) {
  return requestClient.post<{ id: number }>('/toonflow/script/addScript', data);
}

export function updateScript(data: {
  assets: number[];
  content: string;
  id: number;
  name: string;
  shouldGenerateImage?: number;
}) {
  return requestClient.post('/toonflow/script/updateScript', data);
}

export function deleteScripts(ids: number[]) {
  return requestClient.post('/toonflow/script/delScript', { ids });
}

export function getAssets(projectId: number) {
  return requestClient.post<ToonflowApi.Asset[]>('/toonflow/assets/getAssetsApi', {
    projectId,
  });
}

export function getAssetLibrary(projectId?: number) {
  return requestClient.post<ToonflowApi.LibraryAsset[]>('/toonflow/assets/library', {
    projectId,
  });
}

export function linkProjectAsset(projectId: number, assetId: number) {
  return requestClient.post('/toonflow/assets/link', { projectId, assetId });
}

export function unlinkProjectAsset(projectId: number, assetId: number) {
  return requestClient.post('/toonflow/assets/unlink', { projectId, assetId });
}

export function saveAsset(data: Partial<ToonflowApi.Asset> & { name: string; projectId: number }) {
  return requestClient.post<{ id: number }>('/toonflow/assets/saveAssets', data);
}

export function getFlowData(projectId: number, episodesId: number) {
  return requestClient.post<Record<string, any>>('/toonflow/production/getFlowData', {
    projectId,
    episodesId,
  });
}

export function saveFlowData(projectId: number, episodesId: number, data: Record<string, any>) {
  return requestClient.post('/toonflow/production/saveFlowData', {
    projectId,
    episodesId,
    data,
  });
}

export function getStoryboards(projectId: number, scriptId: number) {
  return requestClient.post<ToonflowApi.Storyboard[]>(
    '/toonflow/production/getStoryboardData',
    { projectId, scriptId },
  );
}

export function addStoryboard(data: Partial<ToonflowApi.Storyboard>) {
  return requestClient.post<{ id: number }>('/toonflow/production/storyboard/addStoryboard', data);
}

export function removeStoryboard(id: number) {
  return requestClient.post('/toonflow/production/storyboard/removeFrame', { id });
}

export function editStoryboardInfo(data: Pick<ToonflowApi.Storyboard, 'associateAssetsIds' | 'duration' | 'id' | 'prompt' | 'shouldGenerateImage' | 'track' | 'videoDesc'>) {
  return requestClient.post('/toonflow/production/storyboard/editStoryboardInfo', data);
}

export function reorderStoryboards(projectId: number, scriptId: number, storyboardIds: number[]) {
  return requestClient.post('/toonflow/production/storyboard/reorder', { projectId, scriptId, storyboardIds });
}


export function getAgentDeployments() {
  return requestClient.get<ToonflowApi.AgentDeployment[]>('/toonflow/setting/agentDeploy');
}

export function updateAgentDeployment(data: Partial<ToonflowApi.AgentDeployment> & { id: number }) {
  return requestClient.post('/toonflow/setting/agentDeploy', data);
}

export const getVideoWorkbench = (projectId: number, scriptId: number) => requestClient.post<Record<string, any>>('/production/workbench/getGenerateData', { projectId, scriptId });
export const addVideoTrack = (projectId: number, scriptId: number, duration = 5) => requestClient.post<number>('/production/workbench/addTrack', { projectId, scriptId, duration });
export const updateVideoTrackPrompt = (id: number, prompt: string) => requestClient.post('/production/workbench/updateVideoPrompt', { id, prompt });
export const generateTrackVideo = (data: Record<string, any>) => requestClient.post<number>('/production/workbench/generateVideo', data);
export const generateVideoPrompt = (data: Record<string, any>) => requestClient.post<string>('/production/workbench/generateVideoPrompt', data);
export const batchGenerateVideoPrompts = (data: Record<string, any>) => requestClient.post('/production/workbench/batchGeneratePrompt', data);
export const batchGenerateVideos = (data: Record<string, any>) => requestClient.post('/production/workbench/batchGenerateVideo', data);
export const reorderVideoTracks = (projectId: number, scriptId: number, trackIds: number[]) => requestClient.post('/production/workbench/reorderTracks', { projectId, scriptId, trackIds });
export const bindTrackStoryboards = (trackId: number, storyboardIds: number[]) => requestClient.post('/production/workbench/bindStoryboards', { trackId, storyboardIds });
export const cancelTrackVideo = (id: number) => requestClient.post('/production/workbench/cancelVideo', { id });
export const retryTrackVideo = (data: Record<string, any>) => requestClient.post<number>('/production/workbench/retryVideo', data);
export const pollTrackVideos = (projectId: number, scriptId: number, videoIds: number[]) => requestClient.post<Array<{ id: number; state: string; errorReason?: string; filePath?: string; src?: string }>>('/production/workbench/checkVideoStateList', { projectId, scriptId, videoIds });
export const exportFinalVideo = (projectId: number, scriptId: number) => requestClient.post<{ taskId: number; state: string }>('/production/workbench/exportVideo', { projectId, scriptId });
export const selectTrackVideo = (trackId: number, videoId: number) => requestClient.post('/production/workbench/selectVideo', { trackId, videoId });
export const deleteTrackVideo = (id: number) => requestClient.post('/production/workbench/delVideo', { id });
export const getAudioBindAssetsList = (assetsIds: number[]) => requestClient.post<any[]>('/production/workbench/getAudioBindAssetsList', { assetsIds });
export const getWorkbenchFileUrls = (items: Array<{ id: number; sources: 'assets' | 'storyboard' }>) => requestClient.post<{ data: Record<string, string> }>('/production/workbench/getFileUrl', { items });
export const getAssetsWithAudio = (projectId: number) => requestClient.post<Array<ToonflowApi.Asset & { audioUrl?: string; relepedAudio: Array<{ id: number; name: string; url?: string }> }>>('/cornerScape/getAllAssets', { projectId });
export const updateAssetAudio = (assetsId: number, audioIds: number[]) => requestClient.post('/cornerScape/updateAssetsAudio', { assetsId, audioIds });
export const batchBindAudio = (projectId: number, assetsIds: number[]) => requestClient.post('/cornerScape/batchBindAudio', { projectId, assetsIds });
export const pollAssetAudio = (ids: number[]) => requestClient.post<Array<{ id: number; audioBindState?: number }>>('/cornerScape/pollingAudio', { ids });
export const generateAssetDubbing = (data: { projectId: number; assetsId: number; text: string; voice?: string; model?: string }) => requestClient.post<{ id: number; url: string }>('/cornerScape/generateDubbing', data);
export const runAgent = (data: { agentType: 'productionAgent' | 'scriptAgent'; isolationKey: string; projectId: number; scriptId?: number; content: string; think?: boolean; thinkLevel?: number }) => requestClient.post<{ id: number; state: string; content: string }>('/agents/chat', data);
export const startAgent = (data: { agentType: 'productionAgent' | 'scriptAgent'; isolationKey: string; projectId: number; scriptId?: number; content: string; think?: boolean; thinkLevel?: number }) => requestClient.post<{ id: number; state: string }>('/agents/start', data);
export const getAgentRunState = (id: number) => requestClient.post<ToonflowApi.AgentRun>('/agents/runState', { id });
export const stopAgent = (id: number) => requestClient.post<{ id: number; state: string }>('/agents/stop', { id });
export const getAgentRunEvents = (runId: number, afterId = 0) => requestClient.post<ToonflowApi.AgentRunEvent[]>('/agents/events', { runId, afterId });
export const retryAgent = (id: number) => requestClient.post<{ id: number; state: string; retryOf: number }>('/agents/retry', { id });
export const getAgentMemories = (agentType: 'productionAgent' | 'scriptAgent', isolationKey: string) => requestClient.post<ToonflowApi.AgentMemory[]>('/agents/memories', { agentType, isolationKey });
export const getAgentRuns = (agentType: 'productionAgent' | 'scriptAgent', isolationKey: string) => requestClient.post<ToonflowApi.AgentRun[]>('/agents/runs', { agentType, isolationKey });
export const clearAgentMemory = (agentType: 'productionAgent' | 'scriptAgent', isolationKey: string, memoryType: 'all' | 'message' | 'summary' = 'all') => requestClient.post('/agents/clearMemory', { agentType, isolationKey, memoryType });
export const getScriptAgentPlan = (projectId: number) => requestClient.post<{ id: number; data: { storySkeleton: string; adaptationStrategy: string; script: Array<{ id: number; name: string; content: string }> } }>('/scriptAgent/getPlanData', { projectId, agentType: 'scriptAgent' });
export const saveScriptAgentPlan = (projectId: number, data: Record<string, any>) => requestClient.post<{ id: number }>('/scriptAgent/setPlanData', { projectId, agentType: 'scriptAgent', data });
export const executeAgentTool = (data: { agentType: 'productionAgent' | 'scriptAgent'; projectId: number; scriptId?: number; toolName: string; arguments?: Record<string, any> }) => requestClient.post<{ callId: number; result: any }>('/agents/tools/execute', data);
