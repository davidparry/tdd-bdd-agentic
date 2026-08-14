# The executable behavior spec (BDD level) for the String Calculator kata.
#
# Each scenario is tagged with the requirement it verifies. This is the
# COMPLETE branch: every requirement in requirements/requirements.json has
# been driven through the loop (spec -> Gherkin -> RED -> GREEN -> REFACTOR).
# REQ-007 duplicates REQ-005's behavior, so its scenario carries both tags.
Feature: String Calculator addition
  As a user of the calculator
  I want delimited number strings to be summed safely
  So that any input produces a predictable result

  @REQ-001
  Scenario: An empty string returns zero
    Given a string calculator
    When I add ""
    Then the result is 0

  @REQ-002
  Scenario: A single number returns its value
    Given a string calculator
    When I add "7"
    Then the result is 7

  @REQ-002
  Scenario: Another single number returns its value
    Given a string calculator
    When I add "42"
    Then the result is 42

  @REQ-003
  Scenario: Two numbers separated by a comma are summed
    Given a string calculator
    When I add "1,2"
    Then the result is 3

  @REQ-003
  Scenario: Two larger numbers separated by a comma are summed
    Given a string calculator
    When I add "10,20"
    Then the result is 30

  @REQ-004
  Scenario: Any amount of numbers is summed
    Given a string calculator
    When I add "1,2,3,4,5"
    Then the result is 15

  @REQ-004
  Scenario: All zeros sum to zero
    Given a string calculator
    When I add "0,0,0"
    Then the result is 0

  @REQ-005 @REQ-007
  Scenario: Newlines work as delimiters alongside commas
    Given a string calculator
    When I add "1\n2,3"
    Then the result is 6

  @REQ-005
  Scenario: Newlines alone delimit numbers
    Given a string calculator
    When I add "4\n5\n6"
    Then the result is 15

  @REQ-006
  Scenario: A negative number is rejected
    Given a string calculator
    When I add "1,-2"
    Then an IllegalArgumentException is thrown with a message containing "negatives not allowed"

  @REQ-006
  Scenario: Every negative number is listed in the error
    Given a string calculator
    When I add "-1,-2"
    Then an IllegalArgumentException is thrown with a message containing "-1"
    And an IllegalArgumentException is thrown with a message containing "-2"
