import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import MessageBubble from '../components/MessageBubble.vue'
import type { ChatMessage } from '../types'

function userMsg(content: string, extras: Partial<ChatMessage> = {}): ChatMessage {
  return { role: 'user', content, timestamp: Date.now(), ...extras }
}

function assistantMsg(content: string, extras: Partial<ChatMessage> = {}): ChatMessage {
  return { role: 'assistant', content, timestamp: Date.now(), ...extras }
}

describe('MessageBubble', () => {
  it('renders user message with correct class (right-aligned)', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: userMsg('Hello') },
    })
    expect(wrapper.find('.message-row').classes()).toContain('message-user')
    expect(wrapper.find('.message-bubble').classes()).toContain('user')
    expect(wrapper.find('.msg-avatar').exists()).toBe(false) // no avatar for user
  })

  it('renders assistant message with avatar and class', () => {
    const wrapper = mount(MessageBubble, {
      props: {
        message: assistantMsg('Hi there'),
        agentName: 'Coder',
        agentColor: '#00d4ff',
      },
    })
    expect(wrapper.find('.message-row').classes()).toContain('message-assistant')
    expect(wrapper.find('.message-bubble').classes()).toContain('assistant')
    expect(wrapper.find('.msg-avatar').exists()).toBe(true)
    expect(wrapper.find('.msg-avatar span').text()).toBe('C')
  })

  it('displays message content text', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: userMsg('Test content here') },
    })
    expect(wrapper.find('.message-content').text()).toBe('Test content here')
  })

  it('shows latency badge when latency_ms is present', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: assistantMsg('Reply', { latency_ms: 150 }) },
    })
    expect(wrapper.find('.msg-latency').exists()).toBe(true)
    expect(wrapper.find('.msg-latency').text()).toBe('150ms')
  })

  it('hides latency badge when latency_ms is absent', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: assistantMsg('Reply') },
    })
    expect(wrapper.find('.msg-latency').exists()).toBe(false)
  })

  it('shows sources toggle button when sources are present', () => {
    const sources = [
      { content: 'Source 1', score: 0.9, memory_id: 'mem-1' },
      { content: 'Source 2', score: 0.7, memory_id: 'mem-2' },
    ]
    const wrapper = mount(MessageBubble, {
      props: { message: assistantMsg('Reply', { sources }) },
    })
    const toggle = wrapper.find('.sources-toggle')
    expect(toggle.exists()).toBe(true)
    expect(toggle.text()).toContain('2条')
  })

  it('toggles source panel on click', async () => {
    const sources = [
      { content: 'Source 1', score: 0.9, memory_id: 'mem-1' },
    ]
    const wrapper = mount(MessageBubble, {
      props: { message: assistantMsg('Reply', { sources }) },
    })

    // Initially hidden
    expect(wrapper.find('.source-panel').exists()).toBe(false)

    // Click to show
    await wrapper.find('.sources-toggle').trigger('click')
    expect(wrapper.find('.source-panel').exists()).toBe(true)

    // Click to hide
    await wrapper.find('.sources-toggle').trigger('click')
    expect(wrapper.find('.source-panel').exists()).toBe(false)
  })

  it('hides sources toggle when no sources', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: userMsg('Hello') },
    })
    expect(wrapper.find('.sources-toggle').exists()).toBe(false)
  })

  it('uses default avatar when agentName not provided', () => {
    const wrapper = mount(MessageBubble, {
      props: { message: assistantMsg('Hi') },
    })
    expect(wrapper.find('.msg-avatar span').text()).toBe('A')
  })
})
