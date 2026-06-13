/**
 * TigerScan - Contract Wizard Page
 * 
 * Visual smart contract builder
 */

import { useState } from 'react';
import Head from 'next/head';
import { Container, Typography, Card, CardContent, Grid, TextField, Button, Box, Stepper, Step, StepLabel, Paper, Divider, Select, MenuItem, FormControl, InputLabel, Chip, Alert } from '@mui/material';
import { Code, Add, Save, PlayArrow } from '@mui/icons-material';

interface ContractTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
}

const TEMPLATES: ContractTemplate[] = [
  { id: 'erc20', name: 'TEP-20 Token', description: 'Fungible token standard', category: 'token' },
  { id: 'erc721', name: 'TEP-721 NFT', description: 'Non-fungible token', category: 'nft' },
  { id: 'erc1155', name: 'TEP-1155 Multi', description: 'Multi-token standard', category: 'nft' },
  { id: 'staking', name: 'Staking Pool', description: 'Token staking contract', category: 'defi' },
  { id: 'governance', name: 'DAO Governance', description: 'Voting and proposals', category: 'governance' },
  { id: 'vault', name: 'Timelock Vault', description: 'Time-locked vault', category: 'security' },
];

export default function ContractWizardPage() {
  const [step, setStep] = useState(0);
  const [template, setTemplate] = useState('');
  const [name, setName] = useState('');
  const [symbol, setSymbol] = useState('');
  const [decimals, setDecimals] = useState(18);
  const [initialSupply, setInitialSupply] = useState(0);
  const [owner, setOwner] = useState('');
  const [generated, setGenerated] = useState('');
  
  const steps = ['Select Template', 'Configure', 'Review', 'Deploy'];
  
  const generateContract = () => {
    let code = '';
    
    switch (template) {
      case 'erc20':
        code = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/token/TEP20/TEP20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract ${name || 'MyToken'} is TEP20, Ownable {
    constructor() TEP20("${name || 'MyToken'}", "${symbol || 'MTK'}") {
        _mint(msg.sender, ${initialSupply} * 10 ** decimals);
    }
    
    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }
    
    function burn(uint256 amount) external {
        _burn(msg.sender, amount);
    }
}`;
        break;
      case 'erc721':
        code = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/token/TEP721/TEP721.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract ${name || 'MyNFT'} is TEP721, Ownable {
    constructor() TEP721("${name || 'MyNFT'}", "${symbol || 'MNFT'}") {}
    
    function mint(address to, uint256 tokenId) external onlyOwner {
        _safeMint(to, tokenId);
    }
    
    function batchMint(address to, uint256[] calldata tokenIds) external onlyOwner {
        for (uint256 i = 0; i < tokenIds.length; i++) {
            _safeMint(to, tokenIds[i]);
        }
    }
}`;
        break;
      case 'erc1155':
        code = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/token/TEP1155/TEP1155.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract ${name || 'MyMultiToken'} is TEP1155, Ownable {
    constructor() TEP1155("https://metadata.uri/") {}
    
    function mint(address to, uint256 id, uint256 amount, bytes calldata data) external onlyOwner {
        _mint(to, id, amount, data);
    }
}`;
        break;
      case 'staking':
        code = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/token/TEP20/TEP20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract StakingPool is Ownable {
    TEP20 public stakingToken;
    TEP20 public rewardToken;
    
    mapping(address => uint256) public staked;
    mapping(address => uint256) public rewards;
    
    uint256 public rewardRate = 100; // per second
    
    constructor(address _stakingToken, address _rewardToken) {
        stakingToken = TEP20(_stakingToken);
        rewardToken = TEP20(_rewardToken);
    }
    
    function stake(uint256 amount) external {
        stakingToken.transferFrom(msg.sender, address(this), amount);
        staked[msg.sender] += amount;
        updateReward(msg.sender);
    }
    
    function withdraw(uint256 amount) external {
        require(staked[msg.sender] >= amount, "Insufficient staked");
        staked[msg.sender] -= amount;
        stakingToken.transfer(msg.sender, amount);
        updateReward(msg.sender);
    }
    
    function updateReward(address account) internal {
        rewards[account] += staked[account] * rewardRate;
    }
}`;
        break;
      default:
        code = '// Select a template to get started';
    }
    
    setGenerated(code);
    setStep(3);
  };
  
  return (
    <>
      <Head><title>Contract Wizard | TigerScan</title></Head>
      <Container maxWidth="lg" sx={{ py: 4 }}>
        <Typography variant="h4" gutterBottom><Code sx={{ mr: 1 }} />Contract Wizard</Typography>
        
        <Stepper activeStep={step} sx={{ mb: 4 }}>
          {steps.map(label => <Step key={label}><StepLabel>{label}</StepLabel></Step>)}
        </Stepper>
        
        {step === 0 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Select Template</Typography>
              <Grid container spacing={2}>
                {TEMPLATES.map(t => (
                  <Grid item xs={12} sm={6} md={4} key={t.id}>
                    <Card 
                      variant="outlined" 
                      sx={{ cursor: 'pointer', border: template === t.id ? 2 : 1 }}
                      onClick={() => { setTemplate(t.id); setStep(1); }}
                    >
                      <CardContent>
                        <Typography variant="subtitle1" fontWeight="bold">{t.name}</Typography>
                        <Typography variant="body2" color="text.secondary">{t.description}</Typography>
                        <Chip label={t.category} size="small" sx={{ mt: 1 }} />
                      </CardContent>
                    </Card>
                  </Grid>
                ))}
              </Grid>
            </CardContent>
          </Card>
        )}
        
        {step === 1 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Configure Contract</Typography>
              <Divider sx={{ my: 2 }} />
              <Grid container spacing={3}>
                <Grid item xs={12} md={6}>
                  <TextField fullWidth label="Contract Name" value={name} onChange={(e) => setName(e.target.value)} />
                </Grid>
                <Grid item xs={12} md={6}>
                  <TextField fullWidth label="Symbol" value={symbol} onChange={(e) => setSymbol(e.target.value)} />
                </Grid>
                {['erc20', 'erc721', 'erc1155'].includes(template) && (
                  <Grid item xs={12} md={6}>
                    <FormControl fullWidth>
                      <InputLabel>Decimals</InputLabel>
                      <Select value={decimals} label="Decimals" onChange={(e) => setDecimals(e.target.value as number)}>
                        <MenuItem value={6}>6</MenuItem>
                        <MenuItem value={8}>8</MenuItem>
                        <MenuItem value={18}>18</MenuItem>
                      </Select>
                    </FormControl>
                  </Grid>
                )}
                {template === 'erc20' && (
                  <Grid item xs={12} md={6}>
                    <TextField fullWidth type="number" label="Initial Supply" value={initialSupply} onChange={(e) => setInitialSupply(parseInt(e.target.value))} />
                  </Grid>
                )}
                <Grid item xs={12}>
                  <TextField fullWidth label="Owner Address" value={owner} onChange={(e) => setOwner(e.target.value)} placeholder="0x..." />
                </Grid>
              </Grid>
              <Box sx={{ mt: 3 }}>
                <Button variant="contained" onClick={() => setStep(2)}>Next</Button>
              </Box>
            </CardContent>
          </Card>
        )}
        
        {step === 2 && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Review</Typography>
              <Divider sx={{ my: 2 }} />
              <Grid container spacing={2}>
                <Grid item xs={6}><Typography variant="subtitle2">Template:</Typography></Grid>
                <Grid item xs={6}><Typography>{template}</Typography></Grid>
                <Grid item xs={6}><Typography variant="subtitle2">Name:</Typography></Grid>
                <Grid item xs={6}><Typography>{name}</Typography></Grid>
                <Grid item xs={6}><Typography variant="subtitle2">Symbol:</Typography></Grid>
                <Grid item xs={6}><Typography>{symbol}</Typography></Grid>
                <Grid item xs={6}><Typography variant="subtitle2">Decimals:</Typography></Grid>
                <Grid item xs={6}><Typography>{decimals}</Typography></Grid>
              </Grid>
              <Box sx={{ mt: 3 }}>
                <Button variant="contained" startIcon={<Code />} onClick={generateContract}>Generate Code</Button>
              </Box>
            </CardContent>
          </Card>
        )}
        
        {step === 3 && generated && (
          <Card>
            <CardContent>
              <Typography variant="h6" gutterBottom>Generated Contract</Typography>
              <Divider sx={{ my: 2 }} />
              <Paper variant="outlined" sx={{ p: 2, bgcolor: 'grey.900', maxHeight: 400, overflow: 'auto' }}>
                <Typography component="pre" sx={{ fontFamily: 'monospace', fontSize: '0.75rem', color: 'grey.300', whiteSpace: 'pre-wrap' }}>
                  {generated}
                </Typography>
              </Paper>
              <Box sx={{ mt: 3, display: 'flex', gap: 2 }}>
                <Button variant="contained" startIcon={<Save />}>Save</Button>
                <Button variant="outlined" startIcon={<PlayArrow />}>Deploy</Button>
              </Box>
            </CardContent>
          </Card>
        )}
      </Container>
    </>
  );
}