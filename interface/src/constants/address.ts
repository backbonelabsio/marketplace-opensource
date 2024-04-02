import { NetworkName } from './networks'

type Address = {
  marketplace: string
  nftAddress: string
}

export const addresses: Record<NetworkName, Address> = {
  local: {
    marketplace: '',
    nftAddress: ''
  },
  testnet: {
    marketplace: 'terra1uw266996ycvgg82jlseffcv8758m3u373mtr6k449z4jtgzj4hpqgpch0a',
    nftAddress: 'terra10xl9qmkdehllwful99awqcve4zgz3vn79se2cx6x963yqvzvcgssj64rpt'
  },
  mainnet: {
    marketplace: 'terra1en087uygr8f57vdczvkhy9465t9y6su4ztq4u3',
    nftAddress: ''
  },
}
