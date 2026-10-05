import {Config} from '@remotion/cli/config';

Config.setEntryPoint('./src/index.ts');
Config.setCodec('h264');
Config.setPixelFormat('yuv420p');
Config.setCrf(20);
Config.setVideoImageFormat('png');
Config.setMuted(true);
Config.setConcurrency(2);
Config.setOverwriteOutput(false);
