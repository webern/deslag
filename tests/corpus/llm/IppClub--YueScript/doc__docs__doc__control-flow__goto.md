# Goto

YueScript supports goto statement and label syntax for controlling program flow, following the same rules as Lua's goto statement. **Note:** The goto statement requires Lua 5.2 or higher. When compiling to Lua 5.1, using goto syntax will result in a compilation error.

A label is defined using double colons:

```yuescript
::start::
::done::
::my_label::
```

<YueDisplay>

```yue
::start::
::done::
::my_label::
```

</YueDisplay>

The goto statement jumps to a specified label:

```yuescript
a = 0
::start::
a += 1
goto done if a == 5
goto start
::done::
print "a is now 5"
```

<YueDisplay>

```yue
a = 0
::start::
a += 1
goto done if a == 5
goto start
::done::
print "a is now 5"
```

</YueDisplay>

The goto statement is useful for breaking out of deeply nested loops:

```yuescript
for z = 1, 10
  for y = 1, 10 do for x = 1, 10
    if x^2 + y^2 == z^2
      print 'found a Pythagorean triple:', x, y, z
      goto ok
::ok::
```

<YueDisplay>

```yue
for z = 1, 10
  for y = 1, 10 do for x = 1, 10
    if x^2 + y^2 == z^2
      print 'found a Pythagorean triple:', x, y, z
      goto ok
::ok::
```

</YueDisplay>

You can also use labels to jump to a specific loop level:

```yuescript
for z = 1, 10
  for y = 1, 10
    for x = 1, 10
      if x^2 + y^2 == z^2
        print 'found a Pythagorean triple:', x, y, z
        print 'now trying next z...'
        goto zcontinue
  ::zcontinue::
```

<YueDisplay>

```yue
for z = 1, 10
  for y = 1, 10
    for x = 1, 10
      if x^2 + y^2 == z^2
        print 'found a Pythagorean triple:', x, y, z
        print 'now trying next z...'
        goto zcontinue
  ::zcontinue::
```

</YueDisplay>

## Notes

- Labels must be unique within their scope
- goto can jump to labels at the same or outer scope levels
- goto cannot jump into inner scopes (like inside blocks or loops)
- Use goto sparingly, as it can make code harder to read and maintain
