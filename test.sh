set -e
HOSTNAME=localhost:3000
#HOSTNAME=https://hours4climate.eu


curl --request GET $HOSTNAME/api/country

#curl --request POST \
#    --header "Content-Type: application/json" \
#    --data '{"token":"10000000-aaaa-bbbb-cccc-000000000001", "country": "Denmark", "hours":2.5}' \
#    -o /dev/null \
#    -w "%{http_code}" \
#    $HOSTNAME/api/pledge
#echo ""
echo "GET $HOSTNAME/api/summary"
curl --request GET $HOSTNAME/api/summary
echo ""

echo "GET $HOSTNAME/api/recent"
curl --request GET $HOSTNAME/api/recent
