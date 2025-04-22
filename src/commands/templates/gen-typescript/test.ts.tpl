import { protocol, type TransferParams } from './{{protocol_name}}';

const params: TransferParams ={
  sender: 'addr_test1vpgcjapuwl7gfnzhzg6svtj0ph3gxu8kyuadudmf0kzsksqrfugfc',
  receiver: 'addr_test1vpry6n9s987fpnmjqcqt9un35t2rx5t66v4x06k9awdm5hqpma4pp',
  quantity: 100000000,
};

async function main() {
  try {
    const cbor = await protocol.transferTx(params);
    console.log(cbor);
  } catch (error) {
    console.error('Error:', error);
  }
}

main();
