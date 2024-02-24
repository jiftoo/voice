#!/bin/bash
echo Building voice-waveform-gen
docker build -t voice-waveform-gen --target voice-wave-gen .
echo Building voice-analyzer
docker build -t voice-waveform-gen --target voice-analyzer .
echo Building voice-server
docker build -t voice-waveform-gen --target voice-waveform-gen .
