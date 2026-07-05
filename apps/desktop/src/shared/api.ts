import { invoke } from "@tauri-apps/api/core";
import type { AudioDeviceInfo,ExportFormat,MediaRuntimeStatus,MeetingProject,ProjectDetail,ProviderConfigurationStatus,ProviderDefinition,RealtimeMode,RealtimeStateUpdate,SelectedFile,SoundCheck,SystemAudioStatus,TimelineSegment,ResetVaultStatus } from "./types";
import { LANGUAGE_CODES } from "./languagePreferences";
import { APP_VERSION } from "./appVersion";

interface SetupStatus{vaultExists:boolean;unlocked:boolean}
interface UploadFileInput{selectionToken:string}
const native=Boolean((window as unknown as{__TAURI_INTERNALS__?:unknown}).__TAURI_INTERNALS__);
async function command<T>(name:string,args?:Record<string,unknown>):Promise<T>{if(native)return invoke<T>(name,args);return demoCommand(name,args)as T;}

export const api={
  isNative:native,
  setupStatus:()=>command<SetupStatus>("setup_status"),mediaRuntimeStatus:()=>command<MediaRuntimeStatus>("media_runtime_status"),createVault:(password:string)=>command<SetupStatus>("create_vault",{password}),unlock:(password:string)=>command<SetupStatus>("unlock",{password}),lock:()=>command<SetupStatus>("lock"),resetVaultStatus:()=>command<ResetVaultStatus>("reset_vault_status"),resetVault:(confirmation:string)=>command<SetupStatus>("reset_vault",{confirmation}),
  providerDefinitions:()=>command<ProviderDefinition[]>("provider_definitions"),providerConfigurationStatus:()=>command<ProviderConfigurationStatus[]>("provider_configuration_status"),saveProviderCredentials:(providerId:string,fields:Record<string,unknown>)=>command<void>("save_provider_credentials",{input:{providerId,fields}}),removeProviderSecret:(providerId:string,fieldId:string)=>command<void>("remove_provider_secret",{providerId,fieldId}),removeProviderCredentials:(providerId:string)=>command<void>("remove_provider_credentials",{providerId}),
  loadSettings:()=>command<Record<string,unknown>>("load_settings"),saveSetting:(key:string,value:unknown)=>command<void>("save_setting",{key,value}),selectFiles:(purpose:"meeting_material"|"recording"="meeting_material")=>command<SelectedFile[]>("select_files",{purpose}),audioDevices:()=>command<AudioDeviceInfo[]>("audio_devices"),soundCheck:(deviceId:string)=>command<SoundCheck>("sound_check",{deviceId}),cancelSoundCheck:()=>command<void>("cancel_sound_check"),systemAudioStatus:()=>command<SystemAudioStatus>("system_audio_status"),requestSystemAudioPermission:()=>command<SystemAudioStatus>("request_system_audio_permission"),openSystemAudioSettings:()=>command<void>("open_system_audio_settings"),
  listProjects:()=>command<MeetingProject[]>("list_projects"),getProjectDetail:(projectId:string)=>command<ProjectDetail>("get_project_detail",{projectId}),
  createRealtimeProject:(input:{mode:RealtimeMode;title?:string;deviceId:string;sourceLanguage?:string;translationTargetLanguage?:string;analysisOutputLanguage:string;providerId:string})=>command<ProjectDetail>("create_realtime_project",{input}),pauseRealtime:(projectId:string)=>command<void>("pause_realtime",{projectId}),resumeRealtime:(projectId:string)=>command<void>("resume_realtime",{projectId}),analyzeNow:(projectId:string)=>command<void>("analyze_now",{projectId}),activeRealtimeState:()=>command<RealtimeStateUpdate|null>("active_realtime_state"),showOverlay:(projectId:string)=>command<void>("show_overlay",{projectId}),stopRealtime:(projectId:string)=>command<ProjectDetail>("stop_realtime",{projectId}),
  createUploadProject:(input:{title?:string;files:UploadFileInput[];sourceLanguage?:string;translationTargetLanguage?:string;analysisOutputLanguage:string;minutesOutputLanguage:string;providerId:string})=>command<ProjectDetail>("create_upload_project",{input}),attachUpload:(input:{projectId:string;files:UploadFileInput[];sourceLanguage?:string;translationTargetLanguage?:string;analysisOutputLanguage:string;minutesOutputLanguage:string;providerId:string})=>command<ProjectDetail>("attach_upload",{input}),
  cancelJob:(jobId:string)=>command<void>("cancel_job",{jobId}),retryJob:(jobId:string)=>command<void>("retry_job",{jobId}),regenerateArtifact:(input:{requestId:string;projectId:string;artifactType:string;providerId:string;modelId?:string;outputLanguage:string;sourceSegmentIds:string[];sourceArtifactIds:string[]})=>command<void>("regenerate_artifact",{input}),renameProject:(projectId:string,title:string)=>command<MeetingProject>("rename_project",{projectId,title}),deleteProject:(projectId:string)=>command<void>("delete_project",{projectId}),exportProject:(projectId:string,format:ExportFormat,selectedArtifactIds:string[],includeTranscript:boolean)=>command<string>("export_project",{projectId,format,selectedArtifactIds,includeTranscript}),
};

interface DemoState{vaultCreated:boolean;unlocked:boolean;projects:ProjectDetail[];settings:Record<string,unknown>}
let demo:DemoState={vaultCreated:false,unlocked:false,projects:[],settings:{}};
function demoCommand(name:string,args?:Record<string,unknown>):unknown{
  if(name==="setup_status")return{vaultExists:demo.vaultCreated,unlocked:demo.unlocked};
  if(name==="media_runtime_status")return{available:false,bundled:false,mode:"browser_demo",target:"browser",expectedVersion:"8.1.2",ffmpeg:{available:false,integrityVerified:false,expectedSha256:"",errorCode:"ERR_DESKTOP_REQUIRED"},ffprobe:{available:false,integrityVerified:false,expectedSha256:"",errorCode:"ERR_DESKTOP_REQUIRED"}};
  if(name==="create_vault"){if(String(args?.password??"").length<8)throw"ERR_PASSWORD_TOO_SHORT";if(demo.vaultCreated)throw"ERR_VAULT_ALREADY_EXISTS";demo={...demo,vaultCreated:true,unlocked:true};return{vaultExists:true,unlocked:true};}
  if(name==="unlock"){if(!demo.vaultCreated)throw"ERR_DESKTOP_REQUIRED";demo={...demo,unlocked:true};return{vaultExists:true,unlocked:true};}
  if(name==="lock"){demo={...demo,unlocked:false};return{vaultExists:demo.vaultCreated,unlocked:false};}
  if(name==="reset_vault_status")return{activeRealtimeSessions:0,cleanupPendingSessions:0,activeJobs:0,operationsInFlight:0,resetInProgress:false,recoveryRequired:false,canStart:true,activeWorkBlocksReset:false};
  if(name==="reset_vault"){if(args?.confirmation!=="RESET")throw"ERR_RESET_CONFIRMATION";demo={vaultCreated:false,unlocked:false,projects:[],settings:{}};return{vaultExists:false,unlocked:false};}
  if(name==="provider_definitions")return demoProviders();
  if(!demo.unlocked)throw"ERR_LOCKED";
  if(name==="provider_configuration_status")return[{providerId:"mock",stored:true,configured:true,configuredFields:["scenario"],credentialFieldsConfigured:[],missingRequiredFields:[],configuration:{scenario:"normal"},maskedSummary:"ready"},{providerId:"openai",stored:false,configured:false,configuredFields:[],credentialFieldsConfigured:[],missingRequiredFields:["apiKey"],configuration:{},maskedSummary:"not_configured"},{providerId:"test_adapter",stored:false,configured:false,configuredFields:[],credentialFieldsConfigured:[],missingRequiredFields:[],configuration:{},maskedSummary:"not_configured"}];
  if(name==="save_provider_credentials"){if((args?.input as{providerId:string}).providerId!=="mock")throw"ERR_DESKTOP_REQUIRED";return;}
  if(name==="remove_provider_credentials")return;
  if(name==="load_settings")return demo.settings;
  if(name==="save_setting"){demo.settings={...demo.settings,[String(args?.key)]:args?.value};return;}
  if(name==="select_files"){const purpose=args?.purpose==="recording"?"recording":"meeting_material";return purpose==="recording"?[{selectionToken:crypto.randomUUID(),originalFileName:"demo-recording.wav",kind:"audio",size:256,mimeType:"audio/wav"}]:[{selectionToken:crypto.randomUUID(),originalFileName:"demo-meeting.txt",kind:"transcript",size:128,mimeType:"text/plain"}];}
  if(name==="active_realtime_state")return null;
  if(name==="system_audio_status"||name==="request_system_audio_permission")return{available:true,supported:true,backend:"demo",permissionStatus:"demo",deviceLabel:"Demo system audio",requiresRestart:false};
  if(name==="open_system_audio_settings")return;
  if(name==="cancel_sound_check")return;
  if(["audio_devices","sound_check","create_realtime_project","pause_realtime","resume_realtime","analyze_now","show_overlay","stop_realtime","export_project"].includes(name))throw"ERR_DESKTOP_REQUIRED";
  if(name==="list_projects")return demo.projects.map(v=>v.project).sort((a,b)=>b.createdAt.localeCompare(a.createdAt));
  if(name==="get_project_detail")return demo.projects.find(v=>v.project.id===args?.projectId);
  if(name==="create_upload_project"){const input=args?.input as{title?:string;translationTargetLanguage?:string;analysisOutputLanguage:string;minutesOutputLanguage:string};const detail=demoUpload(input);demo.projects=[detail,...demo.projects];return detail;}
  if(name==="attach_upload"){const input=args?.input as{projectId:string;analysisOutputLanguage:string;minutesOutputLanguage:string};const found=demo.projects.find(v=>v.project.id===input.projectId);if(!found)throw"ERR_PROJECT_NOT_FOUND";const attached=demoAttach(found,input);demo.projects=demo.projects.map(v=>v.project.id===input.projectId?attached:v);return attached;}
  if(name==="rename_project"){const title=String(args?.title??"");demo.projects=demo.projects.map(v=>v.project.id===args?.projectId?{...v,project:{...v.project,title,updatedAt:new Date().toISOString()}}:v);return demo.projects.find(v=>v.project.id===args?.projectId)?.project;}
  if(name==="delete_project"){const found=demo.projects.find(v=>v.project.id===args?.projectId);if(!found)throw"ERR_PROJECT_NOT_FOUND";if(found.project.status==="active")throw"ERR_ACTIVE_SESSION";if(found.project.status==="processing"||found.jobs.some(job=>["queued","running","resumable","cancelling"].includes(job.status)))throw"ERR_ACTIVE_JOB";demo.projects=demo.projects.filter(v=>v.project.id!==args?.projectId);return;}
  if(["cancel_job","retry_job","regenerate_artifact"].includes(name))return;
  throw"ERR_UNSUPPORTED_COMMAND";
}

function demoProviders():ProviderDefinition[]{
  const supportedLanguages=[...LANGUAGE_CODES];
  const capabilities={fileTranscription:true,realtimeTranscription:true,textTranslation:true,segmentUnderstanding:true,meetingSynthesis:true,communicationReview:true,comparisonReport:true,meetingMinutes:true,supportsStreaming:true,supportsStructuredOutput:true,supportsLanguageAutoDetection:true,supportsCodeSwitching:true,supportedInputFormats:["audio","video","transcript","subtitle"],supportedSourceLanguages:["auto",...supportedLanguages],supportedTargetLanguages:supportedLanguages};
  const openAiModelAssignments:ProviderDefinition["modelAssignments"]=[
    {capability:"fileTranscription",configurationFieldId:"transcriptionModel"},
    {capability:"realtimeTranscription",configurationFieldId:"transcriptionModel"},
    {capability:"textTranslation",configurationFieldId:"analysisModel"},
    {capability:"segmentUnderstanding",configurationFieldId:"analysisModel"},
    {capability:"meetingSynthesis",configurationFieldId:"analysisModel"},
    {capability:"communicationReview",configurationFieldId:"analysisModel"},
    {capability:"comparisonReport",configurationFieldId:"analysisModel"},
    {capability:"meetingMinutes",configurationFieldId:"analysisModel"},
  ];
  const testModelAssignments:ProviderDefinition["modelAssignments"]=[
    {capability:"fileTranscription",configurationFieldId:"fileTranscriptionModel"},
    {capability:"realtimeTranscription",configurationFieldId:"realtimeTranscriptionModel"},
    {capability:"textTranslation",configurationFieldId:"textTranslationModel"},
    {capability:"segmentUnderstanding",configurationFieldId:"segmentUnderstandingModel"},
    {capability:"meetingSynthesis",configurationFieldId:"meetingSynthesisModel"},
    {capability:"communicationReview",configurationFieldId:"communicationReviewModel"},
    {capability:"comparisonReport",configurationFieldId:"comparisonReportModel"},
    {capability:"meetingMinutes",configurationFieldId:"meetingMinutesModel"},
  ];
  const testConfigurationSchema=[
    ["fileTranscriptionModel","test-transcribe-ui-v1"],
    ["realtimeTranscriptionModel","test-realtime-ui-v1"],
    ["textTranslationModel","test-translate-ui-v1"],
    ["segmentUnderstandingModel","test-segment-ui-v1"],
    ["meetingSynthesisModel","test-synthesis-ui-v1"],
    ["communicationReviewModel","test-review-ui-v1"],
    ["comparisonReportModel","test-comparison-ui-v1"],
    ["meetingMinutesModel","test-minutes-ui-v1"],
  ].map(([id,defaultValue])=>({id,labelKey:`providers.fields.${id}`,fieldType:"text",required:false,secret:false,defaultValue}));
  return[
    {id:"openai",displayNameKey:"providers.openai.displayName",credentialSchema:[],configurationSchema:[],modelAssignments:openAiModelAssignments,capabilities:{...capabilities,supportsStreaming:false}},
    {id:"mock",displayNameKey:"providers.mock.displayName",credentialSchema:[],configurationSchema:[],modelAssignments:[],capabilities},
    {id:"test_adapter",displayNameKey:"providers.testAdapter.displayName",credentialSchema:[],configurationSchema:testConfigurationSchema,modelAssignments:testModelAssignments,capabilities:{...capabilities,supportsStreaming:false,supportedInputFormats:["ui-only"]}},
  ];
}
function demoUpload(input:{title?:string;translationTargetLanguage?:string;analysisOutputLanguage:string;minutesOutputLanguage:string}):ProjectDetail{const id=crypto.randomUUID(),now=new Date().toISOString();const timeline=[segment(id,0,8100,"The pilot can begin on May 12 if security signs off by the previous Friday."),segment(id,8300,16800,"No public launch date was committed during this meeting."),segment(id,17100,24100,"Legal review timing remains unresolved.")];const artifacts=[artifact(id,"post_meeting_analysis",timeline,{overview:"The meeting addressed conditional pilot timing and unresolved review order.",keyFacts:["The pilot date is conditional."],confirmedDecisions:[],conditions:["Security sign-off is required."],constraints:[],unresolvedIssues:["Legal review timing."],ambiguities:[],recommendedFollowUpActions:["Confirm the review order."],uncertaintyNotes:["No owner or deadline is inferred."],evidenceRefs:[],language:input.analysisOutputLanguage}),artifact(id,"meeting_minutes",timeline,{projectId:id,language:input.minutesOutputLanguage,sourceArtifactIds:[],sections:[{title:"Overview",items:["Discussed pilot timing and review conditions."]}],evidenceRefs:[],limitations:["No owners or deadlines were inferred."]})];return{project:{id,title:input.title||"Demo meeting material",origin:"upload_only",status:"completed",createdAt:now,updatedAt:now,mediaAssetIds:[crypto.randomUUID()],timelineSegmentIds:timeline.map(v=>v.id),artifactIds:artifacts.map(v=>v.id),generationRunIds:[]},timeline,mediaAssets:[{id:crypto.randomUUID(),projectId:id,kind:"transcript",originalFileName:"demo-meeting.txt",importedAt:now,sha256:"demo-only",processingStatus:"ready"}],artifacts,generationRuns:[],jobs:[]}}
function demoAttach(detail:ProjectDetail,input:{analysisOutputLanguage:string;minutesOutputLanguage:string}):ProjectDetail{const comparison=artifact(detail.project.id,"intelligent_comparison_report",detail.timeline,{overallAssessment:"The uploaded source adds the exact approval condition.",correctlyCaptured:[],missedOrIncomplete:[],correctedInterpretations:[],newlyDiscovered:[],guidanceRevisions:[],conclusionChanges:[],recommendedFollowUps:["Confirm security sign-off timing."]});return{...detail,project:{...detail.project,updatedAt:new Date().toISOString(),artifactIds:[...detail.project.artifactIds,comparison.id]},artifacts:[...detail.artifacts,comparison]}}
function segment(projectId:string,startMs:number,endMs:number,sourceTranscript:string):TimelineSegment{return{id:crypto.randomUUID(),projectId,sourceId:"demo-upload",trackRole:"uploaded_media",startMs,endMs,sourceTranscript,detectedLanguage:"en",transcriptStatus:"final",confidence:0.96,warnings:[],createdAt:new Date().toISOString()}}
function artifact(projectId:string,artifactType:string,timeline:TimelineSegment[],payload:unknown){return{id:crypto.randomUUID(),projectId,artifactType,sourceIds:timeline.map(v=>v.id),schemaVersion:`${artifactType}-v1`,promptVersion:`${artifactType}-v1`,providerId:"mock",modelId:"mock-deterministic-v1",appVersion:APP_VERSION,createdAt:new Date().toISOString(),status:"completed",payload}}
