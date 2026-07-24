import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const [sourceRoot, repositoryRoot, outputFile] = process.argv.slice(2);
if (!sourceRoot || !repositoryRoot || !outputFile) {
  throw new Error('usage: node sync-toonflow-content.mjs <Toonflow-app> <kairos-platform> <migration.sql>');
}

const sourceData = path.join(sourceRoot, 'data');
const artRoot = path.join(sourceData, 'skills', 'art_skills');
const storyRoot = path.join(sourceData, 'skills', 'story_skills');
const publicRoot = path.join(
  repositoryRoot,
  'apps/web/apps/web-antd/public/toonflow-resources',
);
const now = 1_784_260_000_000;
const quote = (value) => `'${String(value).replaceAll("'", "''")}'`;
const read = (file) =>
  fs.existsSync(file)
    ? fs.readFileSync(file, 'utf8').replaceAll(/[\t ]+$/gm, '')
    : '';
const directories = (root) =>
  fs.readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
const title = (markdown, fallback) => {
  const first =
    markdown
      .split(/\r?\n/)
      .find((line) => line.trim() && !/^---+$/.test(line.trim())) ?? fallback;
  return first.replace(/^#+\s*/, '').replaceAll('--', '').trim() || fallback;
};
const styleDisplayNames = {
  '2D_90s_japanese_anime': '90年代日式动画',
  '2D_chinese_guofeng': '国风二次元',
  '2D_flat_design': '2D扁平设计',
  '2D_mature_urban_romance': '成熟都市言情动画',
  '3D_anime_render': '3D动画渲染',
  '3D_chinese_traditional': '国风3D',
  '3D_clay_stopmotion': '黏土定格动画',
  '3D_guofeng_cyber': '国风赛博3D',
  realpeople_ancient_chinese: '真人古风写实',
  realpeople_modern_city: '真人现代都市影视',
  realpeople_urban_modern: '真人都市写实',
};
const promptDisplayNames = {
  'seedance2Multi-parameterMode': 'Seedance 2.0 多参数视频提示词',
  universalFirstAndLastFrameMode: '通用首尾帧视频提示词',
  'universalMulti-parameterMode': '通用多参数视频提示词',
  'wan2.6Single-imageFirstFrameMode': 'Wan 2.6 单图首帧视频提示词',
};
const promptDescriptions = {
  'seedance2Multi-parameterMode': '适用于 Seedance 2.0 多参数视频生成',
  universalFirstAndLastFrameMode: '适用于需要首帧、尾帧或首尾帧控制的视频生成',
  'universalMulti-parameterMode': '适用于通用多参数视频生成',
  'wan2.6Single-imageFirstFrameMode': '适用于 Wan 2.6 单图首帧视频生成',
};
const promptSourceKeys = {
  'seedance2Multi-parameterMode': 'seedance_2_multi_parameter',
  universalFirstAndLastFrameMode: 'universal_first_last_frame',
  'universalMulti-parameterMode': 'universal_multi_parameter',
  'wan2.6Single-imageFirstFrameMode': 'wan_2_6_single_image_first_frame',
};
const skillDisplayNames = {
  'production_agent_decision.md': '生产 Agent · 决策调度',
  'production_agent_supervision.md': '生产 Agent · 质量监督',
  'production_execution_derive_assets.md': '衍生资产生成',
  'production_execution_director_plan.md': '导演计划执行',
  'production_execution_generate_assets.md': '资产生成',
  'production_execution_storyboard_gen.md': '分镜生成',
  'production_execution_storyboard_panel.md': '分镜面板执行',
  'production_execution_storyboard_table.md': '分镜表执行',
  'production_skills/storyboard_prompt_techniques.md': '分镜提示词技法',
  'production_skills/storyboard_table_techniques.md': '分镜表设计技法',
  'script_agent_decision.md': '剧本 Agent · 决策调度',
  'script_agent_supervision.md': '剧本 Agent · 质量监督',
  'script_execution_adaptation.md': '改编策略制定',
  'script_execution_script.md': '剧本编写',
  'script_execution_skeleton.md': '故事骨架搭建',
};
const imageFiles = (root, group) => {
  const images = path.join(root, group, 'images');
  if (!fs.existsSync(images)) return [];
  return fs.readdirSync(images)
    .filter((file) => /\.(gif|jpe?g|png|svg|webp)$/i.test(file))
    .sort();
};
const manualData = (root, group, fields) =>
  fields.map(({ label, value, subDir }) => ({
    label,
    value,
    data: read(path.join(root, group, subDir ?? '', `${value}.md`)),
  }));

// Keep the public root stable while Vite is running. Replacing the whole tree can
// leave its dev-server watcher serving the SPA fallback for newly recreated files.
fs.mkdirSync(publicRoot, { recursive: true });
for (const [kind, root] of [['art_skills', artRoot], ['story_skills', storyRoot]]) {
  for (const group of directories(root)) {
    const sourceImages = path.join(root, group, 'images');
    if (!fs.existsSync(sourceImages)) continue;
    fs.cpSync(sourceImages, path.join(publicRoot, kind, group), { recursive: true });
  }
}

const visualFields = [
  { label: 'README', value: 'README' },
  { label: '前缀', value: 'prefix' },
  { label: '角色', value: 'art_character', subDir: 'art_prompt' },
  { label: '角色衍生', value: 'art_character_derivative', subDir: 'art_prompt' },
  { label: '道具', value: 'art_prop', subDir: 'art_prompt' },
  { label: '道具衍生', value: 'art_prop_derivative', subDir: 'art_prompt' },
  { label: '场景', value: 'art_scene', subDir: 'art_prompt' },
  { label: '场景衍生', value: 'art_scene_derivative', subDir: 'art_prompt' },
  { label: '分镜', value: 'director_storyboard', subDir: 'driector_skills' },
  { label: '分镜视频', value: 'art_storyboard_video', subDir: 'art_prompt' },
  { label: '技法-导演规划', value: 'director_planning_style', subDir: 'driector_skills' },
  { label: '技法-分镜表设计', value: 'director_storyboard_table_style', subDir: 'driector_skills' },
];
const directorFields = [
  { label: 'README', value: 'README' },
  { label: '导演规划', value: 'director_planning_narrative', subDir: 'driector_skills' },
  { label: '分镜表', value: 'director_storyboard_table_narrative', subDir: 'driector_skills' },
];

const sql = [
  '-- Generated from Toonflow-app by script/sync-toonflow-content.mjs.',
  '-- Keep source Markdown intact; upserts preserve user-created records.',
  "ALTER TABLE toonflow.prompts ADD COLUMN IF NOT EXISTS source_key varchar(128);",
];

directories(artRoot).forEach((group, index) => {
  const readme = read(path.join(artRoot, group, 'README.md'));
  const name = styleDisplayNames[group] ?? title(readme, group);
  const images = imageFiles(artRoot, group);
  const imageUrls = images.map((file) => `/toonflow-resources/art_skills/${group}/${file}`);
  const data = manualData(artRoot, group, visualFields);
  const prefix = read(path.join(artRoot, group, 'prefix.md'));
  sql.push(
    `INSERT INTO toonflow.art_styles(id,name,file_url,label,prompt,create_time) VALUES (${560100 + index},${quote(name)},${quote(imageUrls[0] ?? '')},${quote(group)},${quote(prefix)},${now + index}) ON CONFLICT(id) DO UPDATE SET name=excluded.name,file_url=excluded.file_url,label=excluded.label,prompt=excluded.prompt;`,
    `INSERT INTO toonflow.creative_manuals(kind,name,path,images,data,create_time,update_time) VALUES ('visual',${quote(name)},${quote(group)},${quote(JSON.stringify(imageUrls))}::jsonb,${quote(JSON.stringify(data))}::jsonb,${now + index},${now + index}) ON CONFLICT(kind,path) DO UPDATE SET name=excluded.name,images=excluded.images,data=excluded.data,update_time=excluded.update_time;`,
  );
});

directories(storyRoot).forEach((group, index) => {
  const readme = read(path.join(storyRoot, group, 'README.md'));
  const name = title(readme, group);
  const imageUrls = imageFiles(storyRoot, group).map(
    (file) => `/toonflow-resources/story_skills/${group}/${file}`,
  );
  const data = manualData(storyRoot, group, directorFields);
  sql.push(
    `INSERT INTO toonflow.creative_manuals(kind,name,path,images,data,create_time,update_time) VALUES ('director',${quote(name)},${quote(group)},${quote(JSON.stringify(imageUrls))}::jsonb,${quote(JSON.stringify(data))}::jsonb,${now + 100 + index},${now + 100 + index}) ON CONFLICT(kind,path) DO UPDATE SET name=excluded.name,images=excluded.images,data=excluded.data,update_time=excluded.update_time;`,
  );
});

const promptRoot = path.join(sourceData, 'modelPrompt', 'video');
fs.readdirSync(promptRoot).filter((file) => file.endsWith('.md')).sort().forEach((file, index) => {
  const key = path.basename(file, '.md');
  const name = promptDisplayNames[key] ?? key;
  sql.push(
    `INSERT INTO toonflow.prompts(id,name,type,data,use_data,source_key) VALUES (${560300 + index},${quote(name)},'视频生成',${quote(read(path.join(promptRoot, file)))},${quote(promptDescriptions[key] ?? 'Toonflow 视频提示词模板')},${quote(promptSourceKeys[key] ?? key)}) ON CONFLICT(id) DO UPDATE SET name=excluded.name,type=excluded.type,data=excluded.data,use_data=excluded.use_data,source_key=excluded.source_key;`,
  );
});
sql.push("SELECT setval('toonflow.prompts_id_seq', greatest((SELECT coalesce(max(id),1) FROM toonflow.prompts),1), true);");

const skillFiles = [
  ...fs.readdirSync(path.join(sourceData, 'skills'))
    .filter((file) => file.endsWith('.md'))
    .map((file) => path.join(sourceData, 'skills', file)),
  ...fs.readdirSync(path.join(sourceData, 'skills', 'production_skills'))
    .filter((file) => file.endsWith('.md'))
    .map((file) => path.join(sourceData, 'skills', 'production_skills', file)),
].sort();
for (const file of skillFiles) {
  const content = read(file);
  const relative = path.relative(path.join(sourceData, 'skills'), file).replaceAll(path.sep, '/');
  const id = crypto.createHash('sha256').update(relative).digest('hex').slice(0, 24);
  const md5 = crypto.createHash('md5').update(content).digest('hex');
  const name = skillDisplayNames[relative] ?? title(content, path.basename(file, '.md'));
  const type = relative.startsWith('production_') || relative.startsWith('production_skills/')
    ? 'production'
    : 'script';
  sql.push(
    `INSERT INTO toonflow.skill_list(id,md5,path,name,description,embedding,type,create_time,update_time,state,content) VALUES (${quote(id)},${quote(md5)},${quote(relative)},${quote(name)},${quote(`Toonflow-app ${relative}`)},NULL,${quote(type)},${now},${now},1,${quote(content)}) ON CONFLICT(id) DO UPDATE SET md5=excluded.md5,path=excluded.path,name=excluded.name,description=excluded.description,type=excluded.type,update_time=excluded.update_time,state=excluded.state,content=excluded.content;`,
  );
}

fs.writeFileSync(outputFile, `${sql.join('\n\n')}\n`);
console.log(JSON.stringify({
  artStyles: directories(artRoot).length,
  directorManuals: directories(storyRoot).length,
  prompts: fs.readdirSync(promptRoot).filter((file) => file.endsWith('.md')).length,
  skills: skillFiles.length,
}));
