curl --request POST \
    --header "Content-Type: application/json" \
    --data '{"token":"10000000-aaaa-bbbb-cccc-000000000001", "country": "Denmark", "hours":2}' \
    -o /dev/null \
    -w "%{http_code}" \
    localhost:3000/api/pledge
echo ""
echo "GET"
curl --request GET localhost:3000/api/summary
echo ""
