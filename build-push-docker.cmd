docker build -t voice .
docker tag voice cr.yandex/crp8e6bl9tg44qjkbs5j/voice:latest
docker push cr.yandex/crp8e6bl9tg44qjkbs5j/voice:latest
echo "Done"
