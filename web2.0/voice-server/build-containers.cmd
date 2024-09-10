@echo off
echo Building voice-analyzer
docker build -t voice-analyzer --target voice-analyzer .
echo Building voice-waveform-gen
docker build -t voice-waveform-gen --target voice-waveform-gen .
echo Building voice-uri-upload
docker build -t voice-uri-upload --target voice-uri-upload .

echo Pushing
docker tag voice-analyzer cr.yandex/crpkd0jf7p97brcjvubn/voice-analyzer
docker push cr.yandex/crpkd0jf7p97brcjvubn/voice-analyzer

docker tag voice-waveform-gen cr.yandex/crpkd0jf7p97brcjvubn/voice-waveform-gen
docker push cr.yandex/crpkd0jf7p97brcjvubn/voice-waveform-gen

docker tag voice-uri-upload cr.yandex/crpkd0jf7p97brcjvubn/voice-uri-upload
docker push cr.yandex/crpkd0jf7p97brcjvubn/voice-uri-upload