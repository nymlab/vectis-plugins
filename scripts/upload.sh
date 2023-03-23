#!/bin/sh

# set -e
source .env

echo $CHAIN_ID

if ! junod keys show deployer --keyring-backend=test; then
  (
    echo "$UPLOAD_USER_MNEMONICS"
    echo "$UPLOAD_USER_MNEMONICS"
  ) | junod keys add deployer --recover --keyring-backend=test 
fi

#  Check we have one runnint to use a local Juno
if [[ "$CHAIN_ID" == "juno-local" ]]; then

  echo "👀 Checking if you have have a local node setup"
  command -v docker >/dev/null 2>&1 || { echo >&2 "Docker is not installed on your machine, local Juno node can't be ran. Install it from here: https://www.docker.com/get-started"; exit 1; }
  NODE_1=`docker ps -a --format="{{.Names}}" | grep "juno_local" | awk '{print $1}'`
  
  if [[ "$NODE_1" == "" ]]; then 
  	echo "Node not found"
  	exit 1
  fi 
fi
  
REST=$(junod tx wasm store artifacts/cronkitty.wasm --gas=auto --gas-prices=0.025ujunox --gas-adjustment=1.3 --from=deployer --keyring-backend=test --chain-id=$CHAIN_ID -o=json -y -b sync --node=$RPC_URL)

echo "Got tx hash"
echo $REST

sleep 25

RESQ=$(junod q tx --node=$RPC_URL --type=hash $(echo "$REST"| jq -r '.txhash') -o json)

echo $RESQ

CODE_ID=$(echo "$RESQ" | jq -r '.logs[0].events[]| select(.type=="store_code").attributes[]| select(.key=="code_id").value')

CODE_HASH=$(echo "$RESQ" | jq -r '.logs[0].events[]| select(.type=="store_code").attributes[]| select(.key=="code_checksum").value')

echo "Code id:"
echo $CODE_ID 

echo "Code hash:"
echo $CODE_HASH



