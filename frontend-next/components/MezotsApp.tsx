'use client'

import { FormEvent, useEffect, useMemo, useState } from 'react'
import { createClient } from '@/lib/supabase/client'

type Profile = { id: string; display_name: string; handle: string; avatar_color: string }
type Room = { id: string; slug: string; name: string; description: string; visibility: string; created_by: string; last_activity_at: string }
type Message = { id: string; room_id: string; user_id: string; body: string; created_at: string; profile?: Profile }

const supabase = createClient()

export default function MezotsApp() {
  const [user, setUser] = useState<any>(null)
  const [profile, setProfile] = useState<Profile | null>(null)
  const [rooms, setRooms] = useState<Room[]>([])
  const [activeRoom, setActiveRoom] = useState<Room | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [view, setView] = useState<'login' | 'signup' | 'app'>('login')
  const [authBusy, setAuthBusy] = useState(false)
  const [authError, setAuthError] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [message, setMessage] = useState('')
  const [roomQuery, setRoomQuery] = useState('')
  const [userQuery, setUserQuery] = useState('')
  const [people, setPeople] = useState<Profile[]>([])

  useEffect(() => {
    supabase.auth.getUser().then(({ data }) => {
      if (data.user) { setUser(data.user); setView('app') }
    })
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setUser(session?.user ?? null)
      setView(session?.user ? 'app' : 'login')
    })
    return () => data.subscription.unsubscribe()
  }, [])

  useEffect(() => {
    if (!user) return
    loadWorkspace()
  }, [user])

  async function loadWorkspace() {
    const [{ data: profileData }, { data: roomData }, { data: peopleData }] = await Promise.all([
      supabase.from('profiles').select('id,display_name,handle,avatar_color').eq('id', user.id).single(),
      supabase.from('rooms').select('*').order('last_activity_at', { ascending: false }),
      supabase.from('profiles').select('id,display_name,handle,avatar_color').order('display_name'),
    ])
    setProfile(profileData)
    setRooms(roomData ?? [])
    setPeople(peopleData ?? [])
    if (!activeRoom && roomData?.[0]) selectRoom(roomData[0])
  }

  async function selectRoom(room: Room) {
    setActiveRoom(room)
    await supabase.from('room_members').upsert({ room_id: room.id, user_id: user.id })
    const { data } = await supabase.from('messages').select('id,room_id,user_id,body,created_at,profile:profiles(id,display_name,handle,avatar_color)').eq('room_id', room.id).order('created_at')
    setMessages((data ?? []).map((item: any) => ({ ...item, profile: Array.isArray(item.profile) ? item.profile[0] : item.profile })))
  }

  useEffect(() => {
    if (!activeRoom) return
    const channel = supabase.channel(`room-${activeRoom.id}`).on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'messages', filter: `room_id=eq.${activeRoom.id}` }, async (payload) => {
      const { data } = await supabase.from('profiles').select('id,display_name,handle,avatar_color').eq('id', payload.new.user_id).single()
      setMessages((current) => current.some((item) => item.id === payload.new.id) ? current : [...current, { ...(payload.new as Message), profile: data ?? undefined }])
    }).subscribe()
    return () => { supabase.removeChannel(channel) }
  }, [activeRoom])

  async function submitAuth(event: FormEvent) {
    event.preventDefault(); setAuthBusy(true); setAuthError('')
    const result = view === 'signup'
      ? await supabase.auth.signUp({ email, password, options: { data: { display_name: displayName }, emailRedirectTo: process.env.NEXT_PUBLIC_DEV_SUPABASE_REDIRECT_URL ?? `${window.location.origin}/auth/callback` } })
      : await supabase.auth.signInWithPassword({ email, password })
    if (result.error) setAuthError(result.error.message.includes('confirm') ? 'Check your email to confirm your account.' : 'Invalid email or password.')
    else if (view === 'signup' && !result.data.session) setAuthError('Account created. Check your email to confirm your account, then sign in.')
    setAuthBusy(false)
  }

  async function sendMessage(event: FormEvent) {
    event.preventDefault(); const body = message.trim(); if (!body || !activeRoom) return
    setMessage('')
    await supabase.from('messages').insert({ room_id: activeRoom.id, user_id: user.id, body })
  }

  async function createRoom() {
    const name = window.prompt('Name your public room')?.trim(); if (!name) return
    const slug = `${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${Math.random().toString(36).slice(2, 6)}`
    const { data } = await supabase.from('rooms').insert({ name, slug, description: 'A new public space on Mezots', visibility: 'public', created_by: user.id }).select().single()
    if (data) { setRooms((current) => [data, ...current]); selectRoom(data) }
  }

  const filteredRooms = useMemo(() => rooms.filter((room) => `${room.name} ${room.description}`.toLowerCase().includes(roomQuery.toLowerCase())), [rooms, roomQuery])
  const filteredPeople = useMemo(() => people.filter((person) => `${person.display_name} ${person.handle}`.toLowerCase().includes(userQuery.toLowerCase())), [people, userQuery])

  if (view !== 'app') return <AuthScreen mode={view} setView={setView} email={email} setEmail={setEmail} password={password} setPassword={setPassword} displayName={displayName} setDisplayName={setDisplayName} submitAuth={submitAuth} busy={authBusy} error={authError} />

  return <main className="min-h-screen bg-[#111318] text-[#f3f4f6]">
    <header className="flex h-16 items-center justify-between border-b border-white/10 px-6"><div className="flex items-center gap-3"><div className="grid size-9 place-items-center rounded-xl bg-[#d9f99d] text-lg font-black text-[#17220d]">m</div><div><p className="font-semibold tracking-tight">Mezots</p><p className="text-[11px] text-white/45">A living room for everyone online</p></div></div><div className="flex items-center gap-3"><span className="hidden text-sm text-white/55 sm:block">Signed in as <strong className="text-white">{profile?.display_name ?? 'Student'}</strong></span><button onClick={() => supabase.auth.signOut()} className="rounded-full border border-white/15 px-3 py-1.5 text-xs text-white/70 transition hover:bg-white/10">Sign out</button></div></header>
    <div className="mx-auto grid min-h-[calc(100vh-4rem)] max-w-[1500px] grid-cols-1 lg:grid-cols-[270px_minmax(0,1fr)_270px]">
      <aside className="border-b border-white/10 p-5 lg:border-b-0 lg:border-r"><div className="mb-5 flex items-center justify-between"><p className="text-xs font-semibold uppercase tracking-[.18em] text-white/40">Public rooms</p><button onClick={createRoom} className="rounded-lg bg-white/10 px-2.5 py-1 text-lg leading-none hover:bg-white/15" aria-label="Create public room">+</button></div><input value={roomQuery} onChange={(e) => setRoomQuery(e.target.value)} placeholder="Search rooms" className="mb-4 w-full rounded-xl border border-white/10 bg-white/[.04] px-3 py-2 text-sm outline-none placeholder:text-white/30 focus:border-[#d9f99d]/60"/><div className="flex flex-col gap-1">{filteredRooms.map((room) => <button key={room.id} onClick={() => selectRoom(room)} className={`rounded-xl px-3 py-3 text-left transition ${activeRoom?.id === room.id ? 'bg-[#d9f99d] text-[#17220d]' : 'text-white/65 hover:bg-white/[.06] hover:text-white'}`}><p className="text-sm font-semibold">{room.name}</p><p className={`mt-1 text-xs ${activeRoom?.id === room.id ? 'text-[#17220d]/65' : 'text-white/35'}`}>{room.description}</p></button>)}</div><div className="mt-8 rounded-2xl border border-[#d9f99d]/20 bg-[#d9f99d]/[.06] p-4"><p className="text-sm font-semibold text-[#d9f99d]">Global by default</p><p className="mt-2 text-xs leading-5 text-white/55">Anyone online can discover and join public rooms. Devices on the same LAN use the same shared room and message feed.</p></div></aside>
      <section className="flex min-h-[620px] flex-col"><div className="border-b border-white/10 px-6 py-5"><div className="flex items-start justify-between gap-4"><div><p className="text-xs uppercase tracking-[.18em] text-[#d9f99d]">Live room</p><h1 className="mt-1 text-2xl font-semibold tracking-tight">{activeRoom?.name ?? 'Choose a room'}</h1><p className="mt-1 text-sm text-white/45">{activeRoom?.description}</p></div><span className="rounded-full border border-[#d9f99d]/30 px-3 py-1 text-xs text-[#d9f99d]">Public · live</span></div></div><div className="flex-1 overflow-y-auto px-6 py-6"><div className="mx-auto flex max-w-2xl flex-col gap-5">{messages.length === 0 ? <div className="rounded-2xl border border-dashed border-white/15 p-8 text-center"><p className="font-medium">You are early.</p><p className="mt-1 text-sm text-white/45">Say hello and everyone in this room will see it.</p></div> : messages.map((item) => <article key={item.id} className="flex gap-3"><div className="grid size-9 shrink-0 place-items-center rounded-full text-xs font-bold text-[#111318]" style={{ backgroundColor: item.profile?.avatar_color ?? '#d9f99d' }}>{(item.profile?.display_name ?? 'S')[0]}</div><div><div className="flex items-baseline gap-2"><strong className="text-sm">{item.profile?.display_name ?? 'Student'}</strong><span className="text-xs text-white/30">@{item.profile?.handle}</span><time className="text-xs text-white/25">{new Date(item.created_at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</time></div><p className="mt-1 text-[15px] leading-6 text-white/75">{item.body}</p></div></article>)}</div></div><form onSubmit={sendMessage} className="border-t border-white/10 p-5"><div className="mx-auto flex max-w-2xl items-center gap-3 rounded-2xl border border-white/10 bg-white/[.04] p-2 focus-within:border-[#d9f99d]/50"><input value={message} onChange={(e) => setMessage(e.target.value)} placeholder={`Message ${activeRoom?.name ?? 'this room'}...`} className="min-w-0 flex-1 bg-transparent px-3 py-2 text-sm outline-none placeholder:text-white/30" maxLength={4000}/><button className="rounded-xl bg-[#d9f99d] px-4 py-2 text-sm font-semibold text-[#17220d] transition hover:bg-[#c9ea8b]">Send</button></div></form></section>
      <aside className="border-t border-white/10 p-5 lg:border-l lg:border-t-0"><p className="text-xs font-semibold uppercase tracking-[.18em] text-white/40">Find people</p><p className="mt-2 text-sm text-white/45">Search by display name or handle.</p><input value={userQuery} onChange={(e) => setUserQuery(e.target.value)} placeholder="Quiet Student, @ss..." className="mt-4 w-full rounded-xl border border-white/10 bg-white/[.04] px-3 py-2 text-sm outline-none placeholder:text-white/30 focus:border-[#d9f99d]/60"/><div className="mt-4 flex flex-col gap-2">{filteredPeople.map((person) => <div key={person.id} className="flex items-center gap-3 rounded-xl p-2 hover:bg-white/[.05]"><div className="grid size-8 place-items-center rounded-full text-xs font-bold text-[#111318]" style={{ backgroundColor: person.avatar_color }}>{person.display_name[0]}</div><div className="min-w-0"><p className="truncate text-sm font-medium">{person.display_name}</p><p className="truncate text-xs text-white/35">@{person.handle}</p></div></div>)}</div></aside>
    </div>
  </main>
}

function AuthScreen({ mode, setView, email, setEmail, password, setPassword, displayName, setDisplayName, submitAuth, busy, error }: any) { return <main className="grid min-h-screen place-items-center bg-[#111318] px-6 text-[#f3f4f6]"><section className="w-full max-w-md"><div className="mb-8 flex items-center gap-3"><div className="grid size-10 place-items-center rounded-xl bg-[#d9f99d] text-xl font-black text-[#17220d]">m</div><div><p className="font-semibold">Mezots</p><p className="text-xs text-white/40">A shared room for real people</p></div></div><div className="rounded-3xl border border-white/10 bg-white/[.04] p-7 shadow-2xl shadow-black/20"><p className="text-xs font-semibold uppercase tracking-[.18em] text-[#d9f99d]">{mode === 'signup' ? 'Join the network' : 'Welcome back'}</p><h1 className="mt-3 text-3xl font-semibold tracking-tight">{mode === 'signup' ? 'Create your identity.' : 'Step into the room.'}</h1><p className="mt-2 text-sm leading-6 text-white/45">{mode === 'signup' ? 'Use a name your new friends will recognize.' : 'Sign in to find rooms and people everywhere.'}</p><form onSubmit={submitAuth} className="mt-7 flex flex-col gap-4">{mode === 'signup' && <label className="text-sm text-white/65">Display name<input required value={displayName} onChange={(e) => setDisplayName(e.target.value)} className="mt-2 w-full rounded-xl border border-white/10 bg-[#111318] px-3 py-3 text-sm text-white outline-none focus:border-[#d9f99d]/60" placeholder="Quiet Student"/></label>}<label className="text-sm text-white/65">Email<input required type="email" value={email} onChange={(e) => setEmail(e.target.value)} className="mt-2 w-full rounded-xl border border-white/10 bg-[#111318] px-3 py-3 text-sm text-white outline-none focus:border-[#d9f99d]/60" placeholder="you@example.com"/></label><label className="text-sm text-white/65">Password<input required minLength={6} type="password" value={password} onChange={(e) => setPassword(e.target.value)} className="mt-2 w-full rounded-xl border border-white/10 bg-[#111318] px-3 py-3 text-sm text-white outline-none focus:border-[#d9f99d]/60" placeholder="At least 6 characters"/></label>{error && <p className="rounded-xl bg-red-400/10 px-3 py-2 text-sm text-red-200">{error}</p>}<button disabled={busy} className="mt-2 rounded-xl bg-[#d9f99d] px-4 py-3 text-sm font-semibold text-[#17220d] disabled:opacity-50">{busy ? 'Working...' : mode === 'signup' ? 'Create account' : 'Sign in'}</button></form><button onClick={() => setView(mode === 'signup' ? 'login' : 'signup')} className="mt-5 w-full text-center text-sm text-white/45 hover:text-white">{mode === 'signup' ? 'Already have an account? Sign in' : 'New here? Create an account'}</button></div><p className="mt-5 text-center text-xs text-white/25">Public rooms are shared globally. Private network access is never assumed.</p></section></main> }
