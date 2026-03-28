import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import EmptyState from '../components/EmptyState.vue'
import { mockAgent } from './test-utils'

describe('EmptyState', () => {
  it('shows agent greeting when agent is provided', () => {
    const wrapper = mount(EmptyState, {
      props: { agent: mockAgent() },
    })
    expect(wrapper.text()).toContain('你好')
    expect(wrapper.text()).toContain('Coder')
    expect(wrapper.find('.empty-greeting').exists()).toBe(true)
  })

  it('shows fallback "UMMS" logo when agent is null', () => {
    const wrapper = mount(EmptyState, {
      props: { agent: null },
    })
    expect(wrapper.find('.empty-logo').exists()).toBe(true)
    expect(wrapper.find('.empty-logo').text()).toBe('UMMS')
    expect(wrapper.text()).toContain('请选择一个智能体开始对话')
  })

  it('generates prompt cards based on agent expertise', () => {
    const wrapper = mount(EmptyState, {
      props: { agent: mockAgent({ expertise: ['rust', 'typescript'] }) },
    })
    const cards = wrapper.findAll('.prompt-card')
    expect(cards.length).toBeGreaterThanOrEqual(2)
    // Should contain expertise-related prompts
    expect(wrapper.text()).toContain('rust')
  })

  it('emits prompt event when prompt card clicked', async () => {
    const wrapper = mount(EmptyState, {
      props: { agent: mockAgent() },
    })
    const card = wrapper.find('.prompt-card')
    await card.trigger('click')
    expect(wrapper.emitted('prompt')).toBeTruthy()
    expect(typeof wrapper.emitted('prompt')![0][0]).toBe('string')
  })

  it('shows agent role and expertise', () => {
    const wrapper = mount(EmptyState, {
      props: {
        agent: mockAgent({ role: '高级工程师', expertise: ['Rust', 'Python'] }),
      },
    })
    expect(wrapper.text()).toContain('高级工程师')
    expect(wrapper.text()).toContain('Rust')
    expect(wrapper.text()).toContain('Python')
  })

  it('shows avatar with first character of agent name', () => {
    const wrapper = mount(EmptyState, {
      props: { agent: mockAgent({ name: 'Writer' }) },
    })
    expect(wrapper.find('.avatar-char').text()).toBe('W')
  })
})
