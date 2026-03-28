import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import ChatWindow from '../components/ChatWindow.vue'
import { mockAgent, mockSession } from './test-utils'
import type { ChatSession } from '../types'

// Mock the API module
vi.mock('../api', () => ({
  sendChat: vi.fn(),
}))

const defaultProps = {
  agentId: 'coder',
  agent: mockAgent(),
  session: mockSession() as ChatSession,
  agentColor: '#00d4ff',
}

describe('ChatWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders empty state when session has no messages', () => {
    const wrapper = mount(ChatWindow, { props: defaultProps })
    // EmptyState component should be rendered
    expect(wrapper.find('.empty-state').exists()).toBe(true)
  })

  it('renders messages when session has messages', () => {
    const session = mockSession({
      messages: [
        { role: 'user', content: 'Hello', timestamp: Date.now() },
        { role: 'assistant', content: 'Hi!', timestamp: Date.now() },
      ],
    }) as ChatSession
    const wrapper = mount(ChatWindow, {
      props: { ...defaultProps, session },
    })
    const bubbles = wrapper.findAll('.message-row')
    expect(bubbles).toHaveLength(2)
  })

  it('disables send button when input is empty', () => {
    const wrapper = mount(ChatWindow, { props: defaultProps })
    const btn = wrapper.find('.send-btn')
    expect(btn.attributes('disabled')).toBeDefined()
  })

  it('enables send button when input has text', async () => {
    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Hello')
    const btn = wrapper.find('.send-btn')
    expect(btn.attributes('disabled')).toBeUndefined()
  })

  it('sends message on Enter key (not Shift+Enter)', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)
    mockedSend.mockResolvedValueOnce({
      message: 'Response',
      agent_id: 'coder',
      sources: [],
      latency_ms: 50,
    })

    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Test message')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: false })

    await flushPromises()

    expect(mockedSend).toHaveBeenCalledWith(
      'coder',
      'Test message',
      expect.any(Array),
    )
    // Session update should be emitted
    expect(wrapper.emitted('update:session')).toBeTruthy()
  })

  it('does NOT send on Shift+Enter', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)

    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Test message')
    await textarea.trigger('keydown', { key: 'Enter', shiftKey: true })

    await flushPromises()
    expect(mockedSend).not.toHaveBeenCalled()
  })

  it('shows ThinkingIndicator during loading', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)
    // Never resolves during this test
    mockedSend.mockReturnValueOnce(new Promise(() => {}))

    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Hello')
    await wrapper.find('.send-btn').trigger('click')

    await flushPromises()

    // ThinkingIndicator should be visible
    expect(wrapper.find('.thinking-indicator, .thinking').exists() ||
      wrapper.html().toLowerCase().includes('thinking')).toBe(true)
  })

  it('renders error message on API failure', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)
    mockedSend.mockRejectedValueOnce(new Error('Network error'))

    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Hello')
    await wrapper.find('.send-btn').trigger('click')

    await flushPromises()

    // Should emit session update with error message
    const emissions = wrapper.emitted('update:session') as Array<[ChatSession]>
    expect(emissions).toBeTruthy()
    const lastSession = emissions[emissions.length - 1][0]
    const lastMessage = lastSession.messages[lastSession.messages.length - 1]
    expect(lastMessage.role).toBe('assistant')
    expect(lastMessage.content).toContain('请求失败')
    expect(lastMessage.content).toContain('Network error')
  })

  it('auto-updates title from first message content', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)
    mockedSend.mockResolvedValueOnce({
      message: 'Got it',
      agent_id: 'coder',
      sources: [],
      latency_ms: 30,
    })

    const session = mockSession({ title: '新对话' }) as ChatSession
    const wrapper = mount(ChatWindow, {
      props: { ...defaultProps, session },
    })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Tell me about Rust memory management')
    await wrapper.find('.send-btn').trigger('click')

    await flushPromises()

    // The first update:session emission should have auto-generated title
    const emissions = wrapper.emitted('update:session') as Array<[ChatSession]>
    expect(emissions[0][0].title).toBe('Tell me about Rust memory m') // first 30 chars
  })

  it('displays agent name in top bar', () => {
    const wrapper = mount(ChatWindow, { props: defaultProps })
    expect(wrapper.find('.top-agent-name').text()).toBe('Coder')
  })

  it('shows character count', async () => {
    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find('.chat-input')
    await textarea.setValue('Hello')
    expect(wrapper.find('.char-count').text()).toBe('5')
  })

  it('shows message count in memory indicator', () => {
    const session = mockSession({
      messages: [
        { role: 'user', content: 'a', timestamp: Date.now() },
        { role: 'assistant', content: 'b', timestamp: Date.now() },
      ],
    }) as ChatSession
    const wrapper = mount(ChatWindow, {
      props: { ...defaultProps, session },
    })
    expect(wrapper.find('.memory-indicator').text()).toContain('2 条消息')
  })

  it('clears input after sending', async () => {
    const { sendChat } = await import('../api')
    const mockedSend = vi.mocked(sendChat)
    mockedSend.mockResolvedValueOnce({
      message: 'ok',
      agent_id: 'coder',
      sources: [],
      latency_ms: 10,
    })

    const wrapper = mount(ChatWindow, { props: defaultProps })
    const textarea = wrapper.find<HTMLTextAreaElement>('.chat-input')
    await textarea.setValue('Hello')
    await wrapper.find('.send-btn').trigger('click')

    await flushPromises()
    expect(textarea.element.value).toBe('')
  })
})
