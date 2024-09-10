@echo off
echo Building voice-analyzer
docker build -t voice-analyzer --target voice-analyzer .
echo Building voice-waveform-gen
docker build -t voice-waveform-gen --target voice-waveform-gen .

echo Pushing
docker tag voice-analyzer cr.yandex/crpkd0jf7p97brcjvubn/voice-analyzer
docker push cr.yandex/crpkd0jf7p97brcjvubn/voice-analyzer

docker tag voice-waveform-gen cr.yandex/crpkd0jf7p97brcjvubn/voice-waveform-gen
docker push cr.yandex/crpkd0jf7p97brcjvubn/voice-waveform-gen