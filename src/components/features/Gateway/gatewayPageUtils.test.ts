import test from 'node:test'
import assert from 'node:assert/strict'
import {
  buildClientSamples,
  buildGatewayBaseUrl,
  buildGatewayConnectHost,
  buildGatewayIntegrationSummary,
  buildGatewaySecuritySummary,
  createGatewayFieldErrors,
  formatGatewayAccountOptionLabel,
  parseClientApiKeys} from './gatewayPageUtils.js'

test('formatGatewayAccountOptionLabel prefers email over verbose label for current account display', () => {
  const label = formatGatewayAccountOptionLabel({
    label: 'Kiro BuilderId 账号',
    email: 'hjj09903+260205210415kv6h@gmail.com',
    userId: 'd-9067642ac7.945864e8-00e1-7095-94d3-eb71ba0e2398',
    id: '0d24370c-1111-2222-3333-444455556666',
    status: 'active'})

  assert.equal(label, 'hjj09903+260205210415kv6h@gmail.com')
})

test('formatGatewayAccountOptionLabel falls back to userId only when email is missing', () => {
  assert.equal(
    formatGatewayAccountOptionLabel({
      label: 'Kiro BuilderId 账号',
      userId: 'builder-user-1',
      id: '0d24370c-1111-2222-3333-444455556666'}),
    'builder-user-1'
  )

  assert.equal(
    formatGatewayAccountOptionLabel({
      label: 'Kiro BuilderId 账号',
      id: '0d24370c-1111-2222-3333-444455556666'}),
    '未知账号'
  )
})

test('createGatewayFieldErrors rejects unsupported host values', () => {
  const errors = createGatewayFieldErrors({
    host: 'bad host',
    port: 8765,
    apiKey: 'sk-test',
    region: 'us-east-1',
    accountMode: 'single',
    accountId: 'account-1',
    groupId: null,
    allowedIpsText: ''})

  assert.equal(errors.host, '监听地址必须是 localhost、IPv4 或 IPv6 地址')
})

test('createGatewayFieldErrors requires allowlist when remote access is enabled', () => {
  const errors = createGatewayFieldErrors({
    host: '0.0.0.0',
    port: 8765,
    apiKey: 'sk-test',
    region: 'us-east-1',
    accountMode: 'single',
    accountId: 'account-1',
    groupId: null,
    localOnly: false,
    allowedIpsText: ''})

  assert.equal(errors.allowedIpsText, '允许远程访问时必须至少配置一个白名单来源 IP')
})

test('buildGatewayConnectHost maps wildcard binds to a usable client host', () => {
  assert.equal(buildGatewayConnectHost('0.0.0.0', true), '127.0.0.1')
  assert.equal(buildGatewayConnectHost('0.0.0.0', false), 'localhost')
  assert.equal(buildGatewayConnectHost('::', false), 'localhost')
})

test('buildGatewayConnectHost falls back to loopback for empty host and passes through explicit binds', () => {
  assert.equal(buildGatewayConnectHost('', true), '127.0.0.1')
  assert.equal(buildGatewayConnectHost('   ', false), '127.0.0.1')
  assert.equal(buildGatewayConnectHost('192.168.1.20', false), '192.168.1.20')
  assert.equal(buildGatewayConnectHost('::1', true), '::1')
})

test('buildGatewayBaseUrl brackets ipv6 addresses for clients', () => {
  assert.equal(buildGatewayBaseUrl('::1', 8765, true), 'http://[::1]:8765')
  assert.equal(buildGatewayBaseUrl('0.0.0.0', 8765, false), 'http://localhost:8765')
})

test('buildClientSamples redacts full api key in integration snippets', () => {
  const samples = buildClientSamples('http://127.0.0.1:8765', 'sk-super-secret-value')

  assert.ok(samples.anthropic.env.includes('ANTHROPIC_API_KEY=sk-'))
  assert.ok(samples.openai.env.includes('OPENAI_API_KEY=sk-'))
  assert.ok(samples.openai.curl.includes('Authorization: Bearer sk-'))
  assert.equal(samples.anthropic.env.includes('sk-super-secret-value'), false)
  assert.equal(samples.openai.env.includes('sk-super-secret-value'), false)
  assert.equal(samples.openai.curl.includes('sk-super-secret-value'), false)
})

test('buildGatewayIntegrationSummary does not expose full bearer token', () => {
  const summary = buildGatewayIntegrationSummary({
    baseUrl: 'http://127.0.0.1:8765',
    clientApiKeysText: 'sk-super-secret-value\nsk-secondary-value',
    logDir: 'C:/tmp/logs',
    errorHistory: []})

  assert.ok(summary.authLabel.startsWith('Bearer sk-'))
  assert.equal(summary.authLabel.includes('sk-super-secret-value'), false)
  assert.equal(summary.authLabel.includes('共 2 个 Key'), true)
})

test('parseClientApiKeys trims, deduplicates and drops blank lines', () => {
  assert.deepEqual(
    parseClientApiKeys(' sk-primary \n\nsk-secondary\nsk-primary \n , sk-third '),
    ['sk-primary', 'sk-secondary', 'sk-third']
  )
})

test('createGatewayFieldErrors requires at least one client api key', () => {
  const errors = createGatewayFieldErrors({
    host: '127.0.0.1',
    port: 8765,
    clientApiKeysText: ' \n ',
    region: 'us-east-1',
    accountMode: 'single',
    accountId: 'account-1',
    groupId: null,
    localOnly: true,
    allowedIpsText: ''})

  assert.equal(errors.clientApiKeysText, '必须至少填写一个客户端 API Key')
})

test('buildGatewaySecuritySummary reports multi-key state', () => {
  const summary = buildGatewaySecuritySummary({
    config: {
      clientApiKeysText: 'sk-primary\nsk-secondary',
      localOnly: false,
      allowedIpsText: '10.0.0.0/24',
      logLevel: 'info'}})

  assert.equal(summary.allowedIpsCount, 1)
  assert.equal(summary.apiKeyState, '已配置 2 个客户端 Key')
})
