# https://leangaurav.medium.com/simplest-https-setup-nginx-reverse-proxy-letsencrypt-ssl-certificate-aws-cloud-docker-4b74569b3c61
upstream default_app {
    server webserver:3000;
}

server {
    listen 80;
    listen [::]:80;
    server_name hours4climate.eu;
    location / {
        return 301 https://$host$request_uri;
    }
    location ~ /.well-known/acme-challenge {
        allow all;
        root /tmp/acme_challenge;
    }
}
server {
    listen 443 ssl;
    listen [::]:443 ssl http2;
    server_name hours4climate.eu;
    ssl_certificate /etc/letsencrypt/live/hours4climate.eu/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/hours4climate.eu/privkey.pem;

    location / {
        proxy_pass http://default_app;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header Host $host;
        proxy_redirect off;
    }
}