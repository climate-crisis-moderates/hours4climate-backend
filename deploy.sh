set -e
cd ~/hours4climate
# build frontend
git clone git@github.com:climate-crisis-moderates/hours4climate-frontend.git | true
# todo: move from origin/main to tag
git fetch && git checkout main && git reset --hard origin/main
cd hours4climate-frontend
# todo: build this somewhere else
./build.sh

cd ~/hours4climate
# load backend
git clone git@github.com:climate-crisis-moderates/hours4climate-backend.git | true
git fetch && git checkout main && git reset --hard $VERSION
cd hours4climate-backend
# todo: use docker-compose and associated files from github artifact or something
docker pull ghcr.io/climate-crisis-moderates/hours4climate-backend

docker-compose up -d
