import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import AgentSelector from '../components/AgentSelector.vue'
import { mockAgent, mockSession } from './test-utils'

const defaultProps = {
  agent: 'coder',
  agents: [
    mockAgent(),
    mockAgent({ agent_id: 'researcher', name: 'Researcher', role: '研究员', expertise: ['data'] }),
  ],
  sessions: [],
  currentSessionId: null,
}

describe('AgentSelector', () => {
  it('renders all agents as cards', () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    const cards = wrapper.findAll('.agent-card')
    expect(cards).toHaveLength(2)
    expect(cards[0].text()).toContain('Coder')
    expect(cards[1].text()).toContain('Researcher')
  })

  it('marks selected agent with active class', () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    const cards = wrapper.findAll('.agent-card')
    expect(cards[0].classes()).toContain('active')
    expect(cards[1].classes()).not.toContain('active')
  })

  it('emits update:agent when agent card clicked', async () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    const cards = wrapper.findAll('.agent-card')
    await cards[1].trigger('click')
    expect(wrapper.emitted('update:agent')).toBeTruthy()
    expect(wrapper.emitted('update:agent')![0]).toEqual(['researcher'])
  })

  it('shows agent avatar with first character uppercase', () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    const avatars = wrapper.findAll('.agent-avatar')
    expect(avatars[0].text()).toBe('C')
    expect(avatars[1].text()).toBe('R')
  })

  it('shows expertise tags (max 3)', () => {
    const agents = [
      mockAgent({ expertise: ['rust', 'python', 'go', 'java'] }),
    ]
    const wrapper = mount(AgentSelector, {
      props: { ...defaultProps, agents },
    })
    const tags = wrapper.findAll('.tag-pill')
    expect(tags).toHaveLength(3) // capped at 3
    expect(tags[0].text()).toBe('rust')
  })

  it('displays sessions filtered by selected agent', () => {
    const sessions = [
      mockSession({ id: 's1', agentId: 'coder', title: 'Rust session' }),
      mockSession({ id: 's2', agentId: 'researcher', title: 'Data session' }),
      mockSession({ id: 's3', agentId: 'coder', title: 'Another session' }),
    ]
    const wrapper = mount(AgentSelector, {
      props: { ...defaultProps, sessions, currentSessionId: 's1' },
    })
    const items = wrapper.findAll('.session-item')
    expect(items).toHaveLength(2) // only coder sessions
    expect(items[0].text()).toContain('Rust session')
  })

  it('emits new-session on new session button click', async () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    await wrapper.find('.new-session-btn').trigger('click')
    expect(wrapper.emitted('new-session')).toBeTruthy()
  })

  it('emits select-session when session item clicked', async () => {
    const sessions = [
      mockSession({ id: 's1', agentId: 'coder', title: 'Test' }),
    ]
    const wrapper = mount(AgentSelector, {
      props: { ...defaultProps, sessions, currentSessionId: null },
    })
    await wrapper.find('.session-item').trigger('click')
    expect(wrapper.emitted('select-session')).toBeTruthy()
    expect(wrapper.emitted('select-session')![0]).toEqual(['s1'])
  })

  it('emits delete-session when delete button clicked (stops propagation)', async () => {
    const sessions = [
      mockSession({ id: 's1', agentId: 'coder', title: 'Test' }),
    ]
    const wrapper = mount(AgentSelector, {
      props: { ...defaultProps, sessions, currentSessionId: 's1' },
    })
    // The delete button is hidden until hover, but it exists in DOM
    const deleteBtn = wrapper.find('.session-delete')
    await deleteBtn.trigger('click')
    expect(wrapper.emitted('delete-session')).toBeTruthy()
    expect(wrapper.emitted('delete-session')![0]).toEqual(['s1'])
    // select-session should NOT be emitted (stopPropagation)
    expect(wrapper.emitted('select-session')).toBeFalsy()
  })

  it('shows empty state when no sessions for agent', () => {
    const wrapper = mount(AgentSelector, { props: defaultProps })
    expect(wrapper.find('.empty-sessions').exists()).toBe(true)
    expect(wrapper.text()).toContain('暂无对话记录')
  })

  it('marks current session with active class', () => {
    const sessions = [
      mockSession({ id: 's1', agentId: 'coder' }),
      mockSession({ id: 's2', agentId: 'coder' }),
    ]
    const wrapper = mount(AgentSelector, {
      props: { ...defaultProps, sessions, currentSessionId: 's2' },
    })
    const items = wrapper.findAll('.session-item')
    expect(items[0].classes()).not.toContain('active')
    expect(items[1].classes()).toContain('active')
  })
})
