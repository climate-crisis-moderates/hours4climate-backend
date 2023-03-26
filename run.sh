export REDIS_HOST_NAME="localhost"
export REDIS_PORT=6379
export REDIS_DB=1
export HCAPTCHA_SECRET="0x0000000000000000000000000000000000000000"
export HTTP_PORT=3000
export STATIC_PATH="./static"
docker build . -t hours4climate-backend_webserver
docker run -it \
    -p "3000:3000" \
    -e REDIS_HOST_NAME='localhost' \
    -e REDIS_PORT=6379 \
    -e REDIS_DB=1 \
    -e HCAPTCHA_SECRET="0x0000000000000000000000000000000000000000" \
    -e HTTP_PORT=3000 \
    -e STATIC_PATH="./static" \
    hours4climate-backend_webserver
